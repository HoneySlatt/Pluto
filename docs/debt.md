# Known debt

What is known to fall short, each item with why it stays and the milestone that follows it. The
PLAN's [*Known debt*](PLAN.md#known-debt) links here. Most of it is in the egui client, the Android
app and the fork, which are frozen and leave at Stage 5
([ADR-0038](adr/0038-a-new-foundation-beside-the-fork.md)). Stage 1's audit triaged the whole list
([`audits/2026-11-18-stage-1.md`](audits/2026-11-18-stage-1.md#debt-triaged)).

## Paid by M1.7, the fixes of Stage 1's audit

- Eight defects of the egui client found by the audit: an image over 8,192 px a side, zoomed in the
  lightbox, exceeds the GPU's texture limit and panics wgpu; switching threads, or quitting, while
  a queued message is edited scrambles or overwrites the drafts; a paste into a terminal can end
  its bracketed paste early and run what follows; the terminal's link detection is quadratic on a
  long line (0.4 s a frame at 40,000 characters); the first emoji and a large paste wait on the
  disk on the interface's thread; a paste is left in `/tmp` readable by others; an image fetch may
  hold 256 MiB twice. The client's text functions are fuzzed, which the audit could not do.
- The drafts are written through a temporary file and a rename with no `fsync`; four of the seven
  write sites ignore their error (`src/app.rs` three times, `crates/pluto-ffi/src/prefs.rs`). M1.7
  makes the drafts' write durable off the interface's thread and those errors visible. The other
  sites (`src/ui/stash.rs`, `src/ui/settings.rs`, `src/app.rs`'s visits and window) leave with the
  egui client; on ext4 with its default options a replace by rename already forces the data out
  first, so a power loss leaves the old file rather than an empty one. The client's settings
  carry no version.
- Agents' pages (`html_render`) and MCP apps load in the client's Chromium with no limit on the
  network: a page's image or `fetch()` reaches any host, private ones included, without a click
  (`src/preview.rs`). M1.7 sets a default-deny Content-Security-Policy, an app's declared domains
  alone allowed.
- CI runs neither the Android app's Kotlin tests nor its APK build
  ([ADR-0012](adr/0012-the-checks-and-ci.md)); `scripts/check.sh --apk` does, locally, and every
  commit touching `android/`, `pluto-ffi` or `pluto-core` runs it (AGENTS.md). M1.7 adds a CI job.
- The table of the vendors' paths and terms that [ADR-0020](adr/0020-the-chat.md) keeps checked at
  each stage's end does not exist. M1.7 writes it.
- The light theme's paths (the `pierre-light` code theme, `Palette.dark == false`, the non-custom
  palette branches) are unreachable since M1.4, and `pierre-light.json` is still loaded at every
  start (`src/highlight.rs`). M1.7 deletes them.
- Mutation testing left 14 mutants alive in `crates/pluto-recording/src/mask.rs` (`strip`,
  `strip_claude`, `strip_grok`) and `wait_for_agent`, where a recording's secrets are masked, and
  its full run (1,300 mutants) was cut. `crates/pluto-measure/tests/runs.rs` returns silently
  without a systemd user manager, so it checks nothing in CI. M1.7 adds the tests that kill them.
- `docs/features.md` speaks in the present of the cut client and cites five files that no longer
  exist. M1.7 brings it to the present.

## Paid by the new foundation (Stage 2 to Stage 5)

- `deny.toml` ignores seven advisories of unmaintained crates with no maintained release their
  parents accept: bincode (syntect), ttf-parser and rustybuzz (resvg 0.45, which egui_extras
  requires), zvariant, zvariant_derive and zbus_names (AccessKit's zbus 5), all still required by
  their newest parents at Stage 1's audit; and chrono, unmaintained since 2026-11-17, a direct
  dependency of the egui client, `pluto-core` and `pluto-ffi`, ignored by the owner's decision of
  2026-11-18. M2.0 chooses jiff or time for the new foundation; the others leave with the egui
  client and the core at Stage 5.
- The ADRs' chains of amendments: each topic is consolidated by the milestone that next decides it,
  as Stage 1's audit plans ([the plan](audits/2026-11-18-stage-1.md#the-adrs-chains)); ADR-0003's
  and ADR-0006's statuses still cite ADR-0011, superseded by ADR-0032, until the engines' topic is
  consolidated at M2.0.
- `AGENTS.md` counts about 1,600 words against about 1,500, `docs/ROADMAP.md` about 5,300; M2.0's
  documents pass shortens them.
- The HP's figures wait for the owner's hand: its commands are in
  [`measurements.md`](measurements.md#the-hp) (M1.2). M2.1 needs its `fsync` time first.

## Leaves with the egui client and the Android app (Stage 5)

- The egui client misses every accessibility need of the bar: its text is painted, about 260
  controls have no name, no focus ring is drawn, a thread is renamed, archived or deleted only by
  the pointer, its primary button's label is at 2.7:1, reduced motion is not followed. The Android
  app's switches and radio buttons do not announce their state and its toasts are not read. The
  uwui client meets the bar ([`uwui-needs.md`](uwui-needs.md)), by the owner's decision of
  2026-11-18.
- The budgets M1.2 found missed ([`measurements.md`](measurements.md)), kept as debt by the owner's
  decision of 2026-11-07: the client at rest, 143 MB against 100 and 3.2 wake-ups a second against
  0; its warm start, 125 ms against 100; the 5,000-message thread, p99 8.5 ms a frame against 4.17;
  the fork at rest, 394 to 892 MB against 80 and 4 to 30 wake-ups a second against 0. M1.4's start
  and Stage 1's audit's figures were taken on a loaded machine and wait for an idle one.
- The core's connection thread wakes at least once a second while connected (`wait` in
  `crates/pluto-core/src/protocol.rs`): 1.1 a second, measured by M1.2. By the owner's decision of
  2026-11-07, it stays until Pluto's own protocol replaces this code.
- AccessKit's D-Bus thread (`blocking-1`, zbus) wakes the client twice a second at rest, the
  largest of its wake-ups (M1.2). It comes with eframe's accessibility, which the screen reader
  needs.
- The egui client still draws 2 frames a minute at rest, its 30-second tick of relative times
  (`src/app.rs`). And egui shortens every delayed repaint by a predicted frame, so a countdown that
  asks again on each frame (a tooltip's delay, a draft's save) ends in a burst of a few frames
  rather than one.
- What is left of a wheel step egui spreads over frames reaches whatever the pointer is over next.
  The timeline drops what began before its thread opened (2026-11-07). The terminal
  (`src/ui/terminal_surface.rs`) and the context menus (`src/ui/context_menu.rs`) read it too and
  may scroll a terminal or menu shown during the glide; not reproduced. A trackpad's kinetic events
  after a thread opens still stop the view following its end.
- Three cuts of the triage stay in the egui client, by the owner's decision of 2026-11-10
  ([ADR-0024](adr/0024-cuts-kept-in-the-egui-client-until-uwui.md)): the action editor
  (`src/ui/action_editor.rs`), the evaluation of the bindings' when-clauses (`src/ui/shortcuts.rs`)
  and the custom model editor's option descriptors (`src/ui/custom_model_editor.rs`).
- A manual connection to the same server does not ask again for the streams the client had open
  (`connect_now` in `src/app.rs`): the sidebar's running-terminal badge, the scheduled tasks and
  the others stay on the old connection. A dropped connection resumes them; a change of server
  does too since M1.4.

## Leaves with the fork (Stage 5)

- The fork claims Claude's banked resets by reading the OAuth token and account file Claude Code
  stores, under Claude Code's own user agent (`apps/server/src/provider/claudeResetCredits.ts`),
  and its Antigravity probe reads that CLI's client secret and refresh token, mints tokens at
  Google and calls Google's internal API as the client (`antigravityUsageLimits.ts`). Both break
  the rule on other programs' credentials, and the providers' terms forbid such intermediation
  ([ADR-0031](adr/0031-three-kinds-of-engine.md)): the risk is the owner's accounts. Kept by the
  owner's decision of 2026-11-18; the Rust server ports neither
  ([ADR-0022](adr/0022-the-triage-row-by-row.md)).
- The fork listens on every address, and the ACP, Pi and Kimi children receive their tokens in
  their environment, readable by the owner's other processes.
- The fork checks the providers' CLIs every 5 minutes, even at rest, and the egui client can no
  longer change the interval (row 19.10 cut its field). The owner chose on 2026-11-10 to keep the
  checks, which show whether each CLI is installed and signed in.
- A Cursor instance already configured on the fork is hidden from the Providers page since M1.4,
  so it can no longer be turned off there, and the fork keeps checking it.
- Pluto's interface shows a native subagent's effort as the parent thread's, not the effort its
  definition sets, which its requests to the model carry.
- The Z Code driver has never run a real turn.
- The rivals' bench found in today's Pluto at rest after 15 threads
  ([`measurements.md`](measurements.md#the-rivals-bench-m16a)): the egui client wakes 3.3 times a
  second since M1.6b1; every Claude Code session the fork started stays alive, about 1.45 GiB and
  325 wake-ups a second for 15; the fork's titles go through Codex, about 54 KB sent per title
  request.

## Paid by Stage 4's engines

- The replay into the fork runs only on the owner's machine, held in CI by a stamp
  ([ADR-0025](adr/0025-agents-sessions-recorded-and-replayed.md)). The scenarios' scripted order
  (approve, then interrupt once the tool runs) and Grok's wake are checked only on synthetic
  recordings until the owner's real ones exist. The agents' CLIs are not pinned in the Nix shell,
  against [ADR-0006](adr/0006-agents-drivers.md): recordings use the CLIs on the owner's PATH, and
  their versions are read from the frames. The roadmap's grid of tests on recorded sessions also
  wants steering, the plan and usage, which no scenario records yet. A Grok recording's prompt is
  compared whole, the fork's instructions included. The scan misses an opaque token longer than
  200 characters under an ordinary key, and a `password=` split across streamed deltas.
- `shellcheck` is not in the Nix shell, so the recording, replay and bench scripts are not linted.

## Paid by the rivals' campaigns (Stage 5)

- The rivals' bench, as M1.6b1's closing review found it: a frozen profile is checked against the
  subject's binaries only, not its setup; a subject still running 20 s after its stop is only
  logged; a run with no window counts with `first_window_ms: none`, which `summary.sh` skips
  silently; a campaign reads the live checkout and `target/release`, and the fingerprint hashes
  neither the fake model nor `pluto-measure`, and checks `src/` for no change; Zed's and T3 Code's
  retries could double a slow prompt; `summary.sh` prints large numbers as `1.343e+04`; a store
  path is parsed three ways (`run.sh`'s `store_name`, `pluto.sh`'s `package_of`, `t3code.sh`).
  None changed a figure of M1.6b1. The branch `m16b2b` carries more of it, unreviewed.
- The rivals' bench counts frames but cannot judge them on time, and has no energy yet
  ([ADR-0026](adr/0026-the-rivals-measured-how.md)).
- The rivals' figures of M1.6a and M1.6b1 are provisional, taken while other sessions loaded the
  machine ([`measurements.md`](measurements.md#the-rivals-bench-m16b1-provisional)).
