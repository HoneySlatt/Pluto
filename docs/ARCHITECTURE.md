# Architecture

The code as it is now: its parts, what depends on what, and how the clients speak to the server.
Where Pluto is going is in [`ROADMAP.md`](ROADMAP.md) and the [ADRs](adr/README.md); this file
changes when the code does.

## The parts

Pluto today is two clients over one shared Rust core. Both speak to one server, the fork
(`pluto-server`, T3 Code's server in TypeScript, outside this repository; see the
[glossary](glossary.md)).

DeepSeek Harness is a provider of the fork ([ADR-0021](adr/0021-deepseek-harness-in-todays-pluto.md)):
the server launches the official `dsh` local Web server and drives its protocol. The clients
receive its models and options through the existing provider snapshots; they never connect to
the Harness process themselves.

| Part | Where | What it is | Size |
|---|---|---|---|
| Desktop client | `src/` | The native client, on egui and eframe, for Linux. Frozen except fixes ([ADR-0010](adr/0010-todays-features-triaged.md)) | 109,000 lines of Rust |
| Shared core | `crates/pluto-core` | The connection to the server, the model of its state, and the timeline built from it. No UI | 11,300 lines |
| Android bindings | `crates/pluto-ffi` | The core wrapped for Kotlin with UniFFI, and the phone's screen data | 9,200 lines |
| Bindings generator | `crates/uniffi-bindgen` | UniFFI's command line, which Gradle runs to write the Kotlin bindings | 3 lines |
| Android app | `android/` | Kotlin and Compose over `pluto-ffi` ([ADR-0002](adr/0002-the-android-app-until-uwui.md)) | 8,100 lines of Kotlin |

What depends on what:

```text
pluto (src/) ─────────────────────────┐
                                      ├──> pluto-core ──> WebSocket and HTTP ──> the fork
android/ ──(JNA)──> pluto-ffi ────────┘
```

`android/` loads `libpluto_ffi.so` through JNA; `pluto-ffi` depends on `pluto-core`.

`pluto-core` depends on no UI crate. It reaches the interface only through `Repaint`, a callback
each client gives it (`crates/pluto-core/src/lib.rs`).

## How the clients speak to the fork

The connection is in `crates/pluto-core/src/protocol.rs`, and the calls outside the socket in
`crates/pluto-core/src/http.rs`, both shared by the two clients. Only the desktop's view of the
server's browser tabs opens a socket of its own (`src/server_browser.rs`). It is T3's protocol,
not yet Pluto's own ([ADR-0005](adr/0005-plutos-own-typed-protocol.md)).

- **Transport.** One WebSocket, `ws(s)://<server>/ws?orchestrationProtocol=2` (`socket_url`),
  through tungstenite and rustls with the system's roots. Plain `ws://` is accepted only towards
  loopback, private and link-local addresses and 100.64.0.0/10; anything else needs TLS.
- **Messages.** Effect RPC's JSON envelopes, not JSON-RPC. The client sends `Request` (an id, a
  method, a payload), `Ack`, `Interrupt` and `Ping`. The server answers with `Chunk` (values of a
  stream, each acknowledged) and `Exit` (success or failure of a request).
- **Authentication.** A bearer token in the `Authorization` header of the WebSocket's upgrade and of
  every HTTP call. The owner issues it on the server (`pluto-server auth session issue`) and pastes
  it in the connection screen, or gives the desktop client a file with `--token-file`. The core
  holds it in a `Zeroizing` string; the desktop never writes it to disk, and the phone seals it with
  a key of the Android Keystore (`Secret.kt`).
- **Opening.** The client subscribes to `subscribeServerConfig`. The first snapshot must announce
  orchestration protocol 2, or the connection fails; it must arrive within 15 seconds. Then the
  client subscribes to the shell (`orchestration.subscribeShell`: projects and threads) and to the
  server's lifecycle.
- **A thread.** Selecting one subscribes to `orchestration.subscribeThread`: a snapshot first, then
  events in sequence, applied to the model (`model.rs`). Older history is fetched over HTTP
  (`GET /api/orchestration/threads/<id>/history`, `http.rs`).
- **Commands.** A message, a new thread or any change goes out as
  `orchestration.dispatchCommand` (builders in `commands.rs`). Other calls are one-shot requests or
  streams (`Action` in `protocol.rs`).
- **Liveness.** A ping every 15 seconds; 45 seconds without a word from the server is a lost
  connection. When the window or the app comes back to the front, a ping must be answered within 6
  seconds. A request without an answer after its threshold is reported as slow (`Event::Slow`),
  so the interface can say a command may have been accepted. Reconnecting is each client's own: the desktop retries with a growing delay
  (`Reconnect` in `src/app.rs`), the phone three times, then shows itself offline
  (`crates/pluto-ffi/src/session.rs`).

### The threads of the core

`Connection::open` starts one thread, `t3-connection`, which owns the socket. It sleeps in `poll()`
on the socket and on an `eventfd` that the interface writes to when it has something to send. It
hands each event to the interface on a channel of 128 entries, then calls `Repaint`. If the
interface falls behind, the channel fills and the worker waits; it does not mistake that wait for a
silent server. Two short-lived threads make the HTTP calls: `t3-auth-session` and `t3-history`.

## The desktop client

- **Start.** `src/main.rs` reads the arguments (`--server`, `--token-file`, `pluto://` links),
  hands a link to an instance already running over a Unix socket (`src/ui/prompt_links.rs`), and
  starts eframe on wgpu.
- **State.** One `App` struct (`src/app.rs`) holds the server's state as `pluto_core::model::State`
  and every screen's own state. Each file of `src/ui/` adds an `impl App` block for its screen:
  there is no layer between the screens and the state.
- **Each frame.** `App::ui` drains the connection's channel (`poll_events`), applies the events,
  then draws. egui redraws only when an input or a `request_repaint` asks it to.
- **Work off the interface's thread.** Network, disk and slow work run on threads that send their
  result back and call `request_repaint`: the core's connection, syntax highlighting
  (`src/highlight.rs`, a worker thread; code shows uncoloured until its colours arrive), the
  preview browser, and short threads started at 23 places in `src/ui/` (attachments, images,
  favicons, diagrams, drafts, search).
- **Other programs.** The client starts some of the machine's own: a Chromium-family browser for
  previews, `ffmpeg` and `ffprobe` for videos, `wl-copy`, `wl-paste` or `xclip` for the clipboard's
  images, `notify-send`, `xdg-open`, and `fc-match` and `fc-list` for fonts.
- **Terminal.** `src/terminal.rs` uses `alacritty_terminal` as an emulator only. The shell runs on
  the server; its input and size go out as `terminal.write` and `terminal.resize`.
- **Preview.** `src/preview.rs` drives a local headless Chromium-family browser over the DevTools
  protocol and paints its screencast frames. `src/server_browser.rs` shows the server's own browser
  tabs, streamed as JPEG frames over a second WebSocket (`/api/preview-stream/ws`). The files of
  the same names in `src/ui/` are their views.
- **Theme.** `src/theme.rs` holds the palette, set from the server's environment themes, with
  Pluto's own as the fallback. Fonts and icons are in `assets/`; their licenses in
  [`NOTICE.md`](../NOTICE.md).
- **Local files.** Settings, window size, drafts and the interface's state are JSON files in
  `$XDG_CONFIG_HOME/pluto/`. Nothing secret is stored there. Besides them: the notifications' icon
  in `$XDG_CACHE_HOME/pluto/`, the socket that receives `pluto://` links in `$XDG_RUNTIME_DIR`, and
  in the temporary directory the preview browser's throwaway profile and pasted images.
- **Two models outside `src/ui/`.** `src/snooze.rs` holds the rules of snoozed threads;
  `src/subagents.rs` builds the roster of a thread's subagents for the Agents panel.

The screens of `src/ui/`, by group:

| Group | Files |
|---|---|
| Connection | `connect` (the connection screen), `requests_panel` (approvals and questions above the composer), `secret_request`, `prompt_links` (`pluto://` links), `quit_hold`, `mod` (the modules and shared helpers) |
| The conversation | `chat`, `chat_export`, `composer*`, `drafts`, `attachments`, `mentions`, `pickers`, `prompt_editor`, `prompt_history`, `typing_guard`, `tasks`, `context_meter`, `timeline_view`, `live_activity`, `minimap`, `markdown`, `md_*`, `cite*`, `citations`, `plan_card`, `queue`, `rewind`, `inspector`, `tool_images`, `tool_icons`, `proactive_panels`, `thread_*` |
| Navigation | `sidebar*`, `legacy_sidebar`, `palette`, `command_menu`, `content_search`, `keybindings`, `when_builder`, `shortcuts`, `toasts`, `lifecycle`, `park`, `undo`, `snooze_dialog`, `context_menu`, `text_menu` |
| Projects | `projects_page`, `project_actions`, `project_file`, `script_import`, `project_grouping`, `project_icon`, `project_scope`, `favicon`, `scratch`, `open_in` |
| Code and Git | `diff_panel`, `diff_review`, `changed_files`, `files`, `file_*`, `branch_picker`, `git_actions`, `source_control`, `stash`, `pull_requests`, `linked_pull_requests`, `bitbucket`, `worktree_setup`, `publish` |
| Panels | `surfaces` (the right panel's launcher), `terminal_drawer`, `terminal_surface`, `preview`, `server_browser`, `html_render`, `mcp_apps`, `agents_panel`, `video`, `media`, `lightbox` |
| Settings | `settings*`, `setting_inheritance`, `providers*` (and `providers/`), `memory_settings`, `usage*`, `scheduled_tasks`, `custom_model_editor`, `action_editor` |
| Chats | `chat_mode`, `chat_projects`, `chat_research`, `chat_image`, `chat_versions` |
| Shared widgets | `widgets`, `typography`, `design*`, `lucide`, `emoji`, `icon_picker`, `accent_picker`, `backdrop`, `pixel_logo` |

## The shared core, `pluto-core`

| Module | What it does |
|---|---|
| `protocol.rs` | The connection, its worker thread and the RPC envelopes (above) |
| `http.rs` | A small blocking HTTP/1.1 client over rustls, for the calls outside the socket |
| `model.rs` | `State`: the shell (projects, threads) and the open thread's detail, updated by events |
| `timeline.rs` | The rows of a conversation, built from its turns: folds, tool groups |
| `tool_presentation.rs` | How a tool call is named and classed |
| `diff.rs` | A unified diff parser, with word-level emphasis |
| `context.rs`, `requests.rs`, `commands.rs`, `chat_requests.rs` | The composer's context records, pending approvals and questions, the commands sent |
| `spaces.rs` | The split between code projects and the chat space |
| `notifications.rs` | Which events of a thread deserve a notification |
| `change_request.rs`, `worktree_setup.rs`, `html_render.rs`, `mcp_app.rs`, `generated_images.rs`, `media.rs` | Smaller pieces each one screen needs: pull request links, worktree setup, published pages, MCP apps, Codex's generated images, image sources |

The desktop client re-exports most of these modules under its own name (`src/lib.rs`).

## The Android app

- **The bindings.** `pluto-ffi` exports one object, `Core`, and about 74 records and enums, with
  UniFFI's macros (no UDL file). It is built as `libpluto_ffi.so` for each ABI by `cargo ndk`;
  Gradle then runs `uniffi-bindgen` on the library to write the Kotlin bindings
  (`android/app/build.gradle.kts`).
- **The state lives in Rust.** `Core` holds the session and every screen's data behind a mutex. A
  thread of its own, `pluto-session`, applies the core's events; every change calls
  `Listener::changed`, a Kotlin callback.
- **The Kotlin side holds no state.** `Model.kt` coalesces `changed()` onto the main thread and bumps
  a revision; Compose screens read it and ask `Core` for their data again. Screens are in
  `android/app/src/main/kotlin/app/pluto/mobile/ui/`.
- **In the background.** `ConnectionService` is a foreground service that keeps the connection, and
  so notifications, alive when the setting asks it (`Notifications.kt`).
- **Building.** `scripts/android-apk.sh` builds the APK in `nix develop .#mobile`; the
  [README](../README.md#android-app-android) has the details.

## Checks, tests, examples and scripts

- `scripts/check.sh` runs every automated check; CI runs it
  ([ADR-0012](adr/0012-the-checks-and-ci.md)).
- Tests: in the modules of each crate, and `tests/ui.rs`, which drives the desktop's screens with
  `egui_kittest` without rendering them, so without a GPU. The Android app has one file of Kotlin unit tests (`LocalNetworkTest.kt`), which
  `scripts/check.sh --apk` runs.
- `examples/bench.rs` measures the cost of each frame against a server, the 5,000-message thread
  and 15 replayed streams among its scenarios; `examples/live.rs` runs the client in a real window
  for the start and the rest; `examples/snapshot.rs` renders the client to a PNG for comparison with
  the reference; `examples/probe.rs` prints what a server sends at connection.
  `examples/tmp_shot.rs` is a leftover of the snapshot work. The examples drive `App` through a few
  public methods marked for benches (`open_thread_titled`, `load_earlier_turns`,
  `load_offline_shell` and their kin, in `src/app.rs`); the client itself never calls them.
- `scripts/measure.sh`, in `nix develop .#measure`, takes the figures of
  [`measurements.md`](measurements.md) ([ADR-0014](adr/0014-how-today-is-measured.md)).
- `scripts/fixture-server.sh` starts a disposable server with synthetic threads
  (`scripts/seed-fixture.mjs`), seeded through T3 Code v0.0.45's server, the flake's `fixture-t3`;
  `scripts/reference/` captures T3's web client for the visual comparison; `scripts/gen-*.mjs`
  generate the icons' sources.

## Limits of the code as it is

- The core uses `eventfd` and `poll()`: it builds for Linux and Android only.
- The connection's thread wakes at least once a second while connected, to check its deadlines and
  send the ping every 15 seconds: 1.1 times a second at rest, measured by M1.2, against the zero
  wake-ups of the quality bar.
- The interface, the core's errors and the server's routes keep T3's names, and many comments cite
  the T3 file a piece was ported from: Pluto was a port until
  [ADR-0003](adr/0003-what-pluto-is-to-become.md).
- The desktop fetches websites' favicons from Google's favicon service, and loads every `https:`
  image that messages show, of its own accord. Private and reserved hosts are never sent. The
  triage keeps only safe images and icons loading by themselves, never through a third party
  ([ADR-0022](adr/0022-the-triage-row-by-row.md)); M1.4 brings the client to it.
- The desktop and `pluto-ffi` each draw their own presentation from the core: the desktop in
  `src/ui/`, the phone in `crates/pluto-ffi/src/views.rs` and Compose. Reconnecting is also written
  twice, with different policies.
