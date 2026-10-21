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
| Timeline | User bubbles with their image attachments (signed asset URLs fetched over HTTP or HTTPS, cropped to their cells; a click opens the expanded preview: fitted image and name, click or wheel zoom up to 8x, drag to pan, arrows or side buttons between images, Escape or a click beside it closes; also from the composer's thumbnails), assistant markdown, settled-turn folds ("Worked for…"), tool rows and group summaries, changed-files card, working indicator, hover meta (time, copy) |
| Markdown | Paragraphs, headings, nested lists, inline code chips, links, quotes (muted text), tables, fenced code with the pierre-dark theme; file references (path-shaped inline code and links to files) as file chips with their file-type icon and "name · parent · L12" labels, opening workspace files in the right panel; external links lead with GitHub's mark, the site's favicon (fetched from Google's favicon service, like upstream; private and reserved hosts are never sent) or a globe |
| Composer | Prompt editor, image attachments (pasted with Ctrl+V, read with `wl-paste` or `xclip` since egui only pastes text, or dropped on the composer, or picked with the attach button through the desktop portal's file chooser; GIF, JPEG, PNG or WebP up to 10 MiB, shown as removable thumbnails and sent as data URLs), model picker, traits and runtime menus, send (`thread.turn.start`), stop, follow-ups during a turn (Stop beside Send; with the "Queue" behavior the message waits in memory as a dashed timeline row with Send now and Cancel, and goes out after the next tool call or when the turn ends, held by approvals and questions; "Steer" sends at once; Stop returns the queue to the composer; off-screen threads send when their turn ends; ported from source, not compared with the reference, which cannot run a turn on the fixture), new-thread drafts (`bootstrap.createThread`), checkout strip (static: drafts always start on the current checkout, without the workspace and branch pickers) |
| Requests | Approval banner (approve/decline) and user-input questions (options, multi-step answers) |
| Right panel | Per-thread panel with the "Open a surface" launcher (letter shortcuts), surface tabs and the "+" surface menu; the Files surface (project tree from `projects.listEntries`, search, read-only highlighted file view with line numbers, wrapping and the tree toggle); the Terminal surface (`terminal.attach` stream into a VT screen with scrollback, alternate screen and 16/256/truecolor SGR, Ghostty's default palette, keyboard and paste through `terminal.write`, resizing, new and close, split groups (side by side or stacked, up to four, the active one bordered), the bottom drawer (Ctrl+J, resizable, its groups sidebar with Single, Side by side and Stacked groups), mouse selection with Copy and Paste (Ctrl+Shift+C, the selection and right-click menus); a locally installed Symbols Nerd Font is used for prompt glyphs); the Agents surface (subagents folded from the thread's `task.*` activities: workflows with their phase rail and phases, direct spawns, the totals footer, or "No agents yet") |
| Diff panel | Working tree, branch and per-turn scopes (`review.getDiffPreview`, `orchestration.getTurnDiff`), unchanged context expanding from the full files on git previews (`review.getDiffFileContents`), the branch scope's base-ref picker (`vcs.listRefs`, local branches paired with their remote), unified or split view, change blocks realigned by similarity and word-alt emphasis like @pierre/diffs, the file tree, collapse, wrap and whitespace toggles, resizable panel |
| Settings | Settings sidebar and header; the scope sentence's project and environment pickers (at a project scope, project-scopable rows read and write the project's `projectSettingsOverrides` entry, show it on the layers icon and reset it; environment-wide rows are inert; descriptions switch to the project wording; Archive lists the project's threads; the repository's t3.json, read once per session with comments and trailing commas tolerated, fills the Workspace and Submodules defaults left unset); the layers button's summary tooltip and inheritance popover (project, environment, t3.json and default layers with the effective one checked; at the environment scope, the projects overriding the row, opening their scope or resetting them; Linux and Mac machines show the monitor icon); the Project page (from the scope, the sidebar's project gear or a thread's "Project settings": the scope hint, renaming, the icon description and reset, "Choose icon" (Lucide icons searched across the whole catalog, emoji, monograms, with their colors; Lucide and emoji project icons are drawn everywhere project icons appear), "Choose file" (the project's image files from `projects.searchEntries`, setting its favicon path; the desktop app's "Open in <file manager>" picker is not ported), project favicons (the server's signed asset URLs, fetched over HTTP or HTTPS), the project's model and workspace defaults, its actions with the Add/Edit Action dialog (icon, name, shortcut written with the keybinding RPCs, command, preview URL, worktree setup options, deletion), importing the scripts the repository's t3.json declares ("Import scripts", and the "t3.json is invalid" notice), and removal after a confirmation); the chat header's project actions (run the last or first action, the actions menu with shortcuts, edit, the t3.json scripts to import and Add action; running opens the bottom drawer and types the command into its active terminal); the General page (new-thread defaults, organization, behavior, projects & threads, confirmations, text generation, about, diagnostics, legacy features) saving server settings with `server.updateSettings`, with the model and traits pickers for the default and text generation models; the Source Control page (repository defaults, the Git row's details with the fetch interval (`backgroundActivity` overrides, reset to the profile default), the Bitbucket row's credentials (access token, or account email and API token; tokens are write-only and never shown), `server.discoverSourceControl` version control and hosting integrations with redacted accounts, text generation style, the writer model switch with its model picker); the Providers page (instance list with status, version and enable switch; the "Add provider instance" wizard (driver, label, instance id, accent color, driver settings; "Continue with ChatGPT" is not ported) and deleting custom instances; the editor's display name with the redacted account, environment variables (name, value, sensitive or plain text; sensitive values are redacted by the server and never shown, a typed replacement is only held until the server's redacted echo), the accent color picker (saturation and brightness plane, hue slider, hex field, Clear color) and the accent badge on the instance marks, runtime fields, models with favorites, visibility and order, adding custom models (slug checks like upstream), editing their name and composer options (driver presets, choices with a default, toggles, "Copy from…" a built-in model, validation) and removing them; the health check interval); the Integrations page (agent browser access, the desktop-only browser defaults, device hub and hosts from `subscribeDeviceState`, read-only); the Storage page (worktree cleanup and retention rules); the SnapShots page (unavailable outside the desktop app, like the web client); the Connections page as a session without `access:write` sees it (administrative access notice, empty remote environments); the Archive page (unarchive, delete); the Keybindings page (shortcut recording and Save, Custom badges, Reset to default and Remove, conflict warnings, the when-clause builder with its expression field and condition tree, adding bindings and the header search); settings search (the sidebar field and "/" over the ported catalog, opening the page, centering the row and pulsing it) |
| Keybindings | The server's resolved bindings drive the global shortcuts with their when clauses (the last match wins): palette modes, Usage and its metric and period keys, new threads, the right panel and diff toggles, closing a surface, previous, next and numbered threads, settle and pin |
| Usage | Header with the metric and period groups and refresh; the Cost and Tokens views (`server.getUsageSummary` merged like upstream: provider summary, daily or hourly chart with hover readout, totals, model and period breakdowns); the Limits view (provider quota windows pooled per driver, share left, pace, reset countdown, `server.refreshProviders`); segment popovers (account with its redacted email, plan, environment, share left, reset time and the share it restores); the environments breadcrumb menu and the Model prices dialog (`usagePriceOverrides` edited per cell, reset to automatic, saved as a settings patch) |
| Pull requests | The page with search, sort, filters (state, involvement, draft, review, checks, project) and provider menus; `pullRequests.list` grouped by involvement and sorted like upstream; rows with state, conflict, checks and review glyphs, line counts, author, repository, labels and update time; loading, empty and error states |
| Provider instances | Display names and the account badge like `providerInstanceDisplay.ts` (accent color, or several instances of one driver) on the sidebar thread rows, the composer and settings model triggers, the model picker rail and the usage account popover |
| State | Shell and thread subscriptions; thread events applied like upstream `threadReducer.ts` |

Not ported yet (buttons for these show a "not implemented" notice):

- settings: the Appearance page (built-in themes, contrast and the other
  appearance controls), the managed ChatGPT account sign-in,
  usage limit hubs, provider update installs and setup flows,
  the keybindings.json file action, device
  setup (enabling the hub or agent access, adding hosts), a second
  terminal when the active one is busy and the script environment variables
  when running actions, Storage's
  project worktree mode (its rows are inert at a project scope), and
  the Connections administration (pairing links, client sessions, network
  access, adding remote environments);
- usage: limit sources, reset credits,
  the environment filter's selection (one environment is always selected) and the model suggestions in Model prices;
- the pull request detail and right panels, searching on the hosts (the
  search filters loaded rows), author and label filters, line-count backfill,
  paging past 99 rows per repository and avatars (initials are shown);
- the surfaces other than Diff, Files, Terminal and Agents (browser, pull requests,
  device), the Agents workflow script viewer (populated Agents rows are ported from source but not compared with the reference: the fixture has no subagents), file editing, previews and "Open in editor", and terminal
  persisting the bottom drawer's groups, the terminal's "Add to chat" and file-path links (URLs open in the browser rather than a preview), and font settings;
- non-image files, downscaling images over 10 MiB, inline image chips in the prompt, attachments in answers to agent questions (a paste with both text and an image keeps the text, where upstream keeps the image), videos and snap-shot details in the expanded preview, file attachments and context chips in user messages, palette
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
fades them in on hover or scroll, and fades the overflowing edge of the question card's options. The usage chart readout has no backdrop blur.

## Development

```sh
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Visual parity is checked on identical synthetic data (the snapshot example
drops every `thread.turn.start`, so it never starts a provider turn):

```sh
scripts/fixture-server.sh                      # disposable server on :3999
ELECTRON=/path/to/electron scripts/reference/shot.sh /tmp/ref.png /
cargo run --example snapshot -- http://127.0.0.1:3999 /tmp/t3-egui-fixture/bearer /tmp/egui.png
```

`scripts/gen-icons.mjs` and `scripts/gen-lang-icons.mjs` regenerate the icon
assets from the upstream packages; `scripts/gen-lucide-catalog.mjs` bundles the
whole Lucide catalog (`assets/lucide/catalog.txt`) for project icons.

Emoji project icons are drawn in color from the system's color emoji font
(found with `fc-match`, as the browser's would be); without one they fall back
to egui's monochrome emoji, and ZWJ sequences show their first emoji.

Fonts: Geist (SIL OFL, `assets/fonts/OFL.txt`). Icons: Lucide (ISC,
`assets/icons/LUCIDE-LICENSE`); the `diffs-symbol-*` file status icons come
from @pierre/diffs (Apache-2.0, `assets/PIERRE-DIFFS-LICENSE`). Code theme:
pierre-dark (MIT, `assets/PIERRE-THEME-LICENSE`).
