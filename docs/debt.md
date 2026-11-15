# Known debt

What is known to fall short, each item with why it stays and what follows it. The PLAN's
[*Known debt*](PLAN.md#known-debt) links here. Most of it is in the egui client and the fork, which
are frozen and leave at Stage 5 ([ADR-0038](adr/0038-a-new-foundation-beside-the-fork.md)).

- The egui client stays at T3 v0.1.0 parity with the gaps listed in `docs/parity/`. Those gaps are
  no longer debt: the triage decided what of them matters ([`features.md`](features.md)).
- The Z Code driver has never run a real turn.
- `deny.toml` ignores six advisories of unmaintained crates with no maintained release their
  parents accept: bincode (syntect), ttf-parser and rustybuzz (resvg 0.45, which egui_extras
  requires), and zvariant, zvariant_derive and zbus_names (AccessKit's zbus 5). Followed by the
  dependency update at Stage 1's end; those of the egui client leave with it.
- The core's connection thread wakes at least once a second while connected
  (`wait` in `crates/pluto-core/src/protocol.rs`), against the bar's zero wake-ups at rest: 1.1 a
  second, measured by M1.2. By the owner's decision of 2026-11-07, it stays until Pluto's own
  protocol replaces this code in Stage 5.
- AccessKit's D-Bus thread (`blocking-1`, zbus) wakes the client twice a second at rest, the
  largest of its wake-ups (M1.2). It comes with eframe's accessibility, which the screen reader
  needs. Followed by Stage 5: the uwui client leaves eframe.
- What is left of a wheel step egui spreads over frames reaches whatever the pointer is over next.
  The timeline drops what began before its thread opened (2026-11-07). The terminal
  (`src/ui/terminal_surface.rs`) and the context menus (`src/ui/context_menu.rs`) read it too and
  may scroll a terminal or menu shown during the glide; not reproduced. A trackpad's kinetic
  events after a thread opens still stop the view following its end. Followed by the uwui client.
- The budgets M1.2 found missed ([`measurements.md`](measurements.md)), kept as debt by the owner's
  decision of 2026-11-07:
  - the client at rest: 143 MB against 100, 3.2 wake-ups a second against 0;
  - its warm start: 125 ms against 100;
  - the 5,000-message thread: p99 8.5 ms a frame against 4.17;
  - the fork at rest: 394 to 892 MB against 80, 4 to 30 wake-ups a second against 0.

  Followed by Stage 5, whose server replaces the fork and whose uwui client replaces the
  egui client. The egui client is frozen and is not tuned meanwhile.
- The fork claims Claude's banked resets by reading the OAuth token Claude Code stores
  (`apps/server/src/provider/claudeResetCredits.ts`), against the rule on other programs'
  credentials. The Rust server does not port it ([ADR-0022](adr/0022-the-triage-row-by-row.md));
  the fork is frozen and changes only on the owner's word. Whether the fork's other quota probes
  read stored credentials is not checked.
- Three cuts of the triage stay in the egui client until the uwui client replaces them, by the
  owner's decision of 2026-11-10 ([ADR-0024](adr/0024-cuts-kept-in-the-egui-client-until-uwui.md)):
  the action editor (`src/ui/action_editor.rs`), the evaluation of the bindings' when-clauses
  (`src/ui/shortcuts.rs`) and the custom model editor's option descriptors
  (`src/ui/custom_model_editor.rs`).
- A manual connection to the same server does not ask again for the streams the client had open
  (`connect_now` in `src/app.rs`): the sidebar's running-terminal badge, the scheduled tasks and
  the others stay on the old connection. A dropped connection resumes them; a change of server
  does too since M1.4. Found by M1.4's review; the egui client gets only fixes, so it waits for
  one.
- Pluto's look is the only one left in the egui client since M1.4, so its light paths (the
  `pierre-light` code theme, `Palette.dark == false`, the non-custom palette branches) are
  unreachable code, though `pierre-light.json` is still loaded at every start
  (`src/highlight.rs`). They leave with the egui client; the uwui client is dark and light
  (row 20.5).
- A Cursor instance already configured on the fork is hidden from the Providers page since M1.4,
  so it can no longer be turned off there, and the fork keeps checking it. It leaves with the
  fork.
- The fork checks the providers' CLIs every 5 minutes, even at rest, and the egui client can no
  longer change the interval (row 19.10 cut its field). The owner chose on 2026-11-10 to keep the
  checks, which show whether each CLI is installed and signed in, until Pluto's own server checks
  on demand.
- Agents' pages (`html_render`) and MCP apps load in the client's Chromium with no limit on the
  network: a page's image or `fetch()` reaches any host, private ones included, without a click.
  Only an MCP app's own host page is intercepted (`src/preview.rs`). Found by M1.4's review; it
  predates M1.4. The thread's sandbox network rule of ADR-0022 waits for Pluto's own server.
- The replay into the fork runs only on the owner's machine, held in CI by a stamp
  ([ADR-0025](adr/0025-agents-sessions-recorded-and-replayed.md)). The scenarios' scripted order
  (approve, then interrupt once the tool runs) and Grok's wake are checked only on synthetic
  recordings until the owner's real ones exist. The agents' CLIs are not pinned in the Nix shell,
  against [ADR-0006](adr/0006-agents-drivers.md): recordings use the CLIs on the owner's PATH, and
  their versions are read from the frames. `shellcheck` is not in the Nix shell, so the scripts
  are not linted. The roadmap's grid of tests on recorded sessions also wants steering, the plan
  and usage, which no scenario records yet. A Grok recording's prompt is compared whole, the
  fork's instructions included. The scan misses an opaque token longer than 200 characters under
  an ordinary key, and a `password=` split across streamed deltas. Followed by Stage 4's first
  engine milestone.
- Pluto's interface shows a native subagent's effort as the parent thread's, not the effort its
  definition sets, which its requests to the model carry. It is the fork's; the fork changes only
  on the owner's word.
- Before M1.5's harness kept its caches outside the fork, two test runs of 2026-11-10 updated the
  fork's own `node_modules/.vite/vitest/<hash>/results.json` and `.vite-temp`, which git ignores.
  Left as they are: they are caches, and the owner's own runs write them too.
- The rivals' bench found in today's Pluto at rest after 15 threads
  ([`measurements.md`](measurements.md#the-rivals-bench-m16a)): the egui client wakes 31 times a
  second in M1.6a, 3.3 since M1.6b1 made the logo's moon still; every Claude Code session the fork started stays alive, about
  1.45 GiB and 325 wake-ups a second for 15; the fork's titles go through Codex, about 54 KB sent
  per title request. The fork leaves in Stage 5; the client is frozen except fixes.
- The egui client still draws 2 frames a minute at rest: its 30-second tick of relative times
  (`src/app.rs`). And egui shortens every delayed repaint by a predicted frame, so a countdown
  that asks again on each frame (a tooltip's delay, a draft's save) ends in a burst of a few
  frames rather than one. Neither runs at rest for long; followed by the uwui client.
- The rivals' bench, as M1.6b1's closing review found it: a frozen profile is checked against the
  subject's binaries only, not its setup; a subject still running 20 s after its stop is only
  logged; a run with no window counts with `first_window_ms: none`, which `summary.sh` skips
  silently; a campaign reads the live checkout and `target/release`, and the fingerprint hashes
  neither the fake model nor `pluto-measure`, and checks `src/` for no change; Zed's and T3 Code's
  retries could double a slow prompt; `summary.sh` prints large numbers as `1.343e+04`; a store
  path is parsed three ways (`run.sh`'s `store_name`, `pluto.sh`'s `package_of`, `t3code.sh`).
  None changed a figure of M1.6b1. Followed by the rivals' campaigns of Stage 5.
- The rivals' bench counts frames but cannot judge them on time, and has no energy yet
  ([ADR-0026](adr/0026-the-rivals-measured-how.md)). Its scripts are not linted: `shellcheck` is
  not in the Nix shell, as for M1.5's.
- CI does not run the Android app's Kotlin tests nor build its APK; `scripts/check.sh --apk` does,
  locally ([ADR-0012](adr/0012-the-checks-and-ci.md)). Followed by Stage 1's audit.
- The HP's figures wait for the owner's hand: its commands are in
  [`measurements.md`](measurements.md#the-hp) (M1.2).
- M1.4's start could not be compared on a loaded machine; it is to be measured again on an idle one.
- The rivals' figures of M1.6a and M1.6b1 are provisional, taken while other sessions loaded the
  machine ([`measurements.md`](measurements.md#the-rivals-bench-m16b1-provisional)); the calm runs
  move to Stage 5 with the other campaigns.
