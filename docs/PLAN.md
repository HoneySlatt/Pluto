# Plan

What is being worked on now: the milestones of the current stage, their status, how each one is
checked, and the known debt. Direction beyond this stage is in [`ROADMAP.md`](ROADMAP.md); decisions
are in [`adr/`](adr/README.md). A *stage* is a step of the roadmap; a *milestone* is a small,
verifiable step of a stage.

Every milestone ends with the definition of done in [`AGENTS.md`](../AGENTS.md#definition-of-done).

## Current

- DeepSeek Harness joined today's Pluto as a provider of the fork, through its official local Web
  protocol ([ADR-0021](adr/0021-deepseek-harness-in-todays-pluto.md)); the owner's delegated tasks
  run on it with real turns since 2026-11-08. Its limits are in the provider guide.
- On 2026-11-06 Pluto came under the owner's method, and changed direction in a design session with
  the owner ([ADR-0003](adr/0003-what-pluto-is-to-become.md) to
  [ADR-0010](adr/0010-todays-features-triaged.md)), and on 2026-11-07 chose a harness of its own
  ([ADR-0011](adr/0011-plutos-own-harness-minimal-and-measured.md)).
- Pluto is no longer a port of T3 Code. `docs/parity/` stays as an inventory.
- Stage 1: [M1.1](#m11--under-the-method), [M1.2](#m12--today-measured) and
  [M1.3](#m13--the-triage) are done, the HP's figures to come by the owner's hand. Work happens
  on `main` since 2026-11-07.
- On 2026-11-08 an interview with the owner set Pluto against the market, the market's features,
  the triage by family, a code editor in Pluto and the chat
  ([ADR-0017](adr/0017-pluto-measured-against-the-market.md) to [ADR-0020](adr/0020-the-chat.md)).
  On 2026-11-10 the owner marked every row of the rethought families
  ([ADR-0022](adr/0022-the-triage-row-by-row.md)), and the
  [specification](specification.md) followed. On 2026-11-10 [M1.4](#m14--the-cuts) removed what it
  cut from the egui client and the Android app, except three cuts kept until the uwui client
  ([ADR-0024](adr/0024-cuts-kept-in-the-egui-client-until-uwui.md)). On 2026-11-11
  [M1.5](#m15--agents-sessions-recorded) built the recorder and the replay into the fork
  ([ADR-0025](adr/0025-agents-sessions-recorded-and-replayed.md)); two of its eleven real
  recordings are made, and the owner set the nine left aside on 2026-11-11. On 2026-11-11 a
  design session split [M1.6](#m16--the-rivals-measured) in three and decided how the rivals are
  measured ([ADR-0026](adr/0026-the-rivals-measured-how.md));
  [M1.6a](#m16a--the-fake-model-and-the-rivals-bench) built the fake model and the bench, and
  took first figures of Pluto, T3 Code and Zed at rest before the owner stopped it. The owner
  set the rivals' measures aside until a go of theirs
  ([ADR-0027](adr/0027-the-rivals-measures-set-aside.md)): M1.6b and M1.6c wait. Next: Stage 1's
  audit, then Stage 2's design session, on the owner's go.
- What Pluto asks of uwui is in [`uwui-needs.md`](uwui-needs.md), for SlimeEngine's M6d.9a, M6d.9b
  and M6d.10.

## Status

| Milestone | Goal | Status |
|---|---|---|
| [M1.1](#m11--under-the-method) | Every check in one command and in CI; the code's architecture written down | Done |
| [M1.2](#m12--today-measured) | Today's client, server and HP measured; the fork's database understood | Done; the HP's figures to come |
| [M1.3](#m13--the-triage) | The owner has triaged every feature; the specification exists | Done |
| [M1.4](#m14--the-cuts) | The egui client and the Android app lose what the triage cuts | Done; its start to measure again on an idle machine |
| [M1.5](#m15--agents-sessions-recorded) | Recorded agent sessions that drivers' tests can replay | Built; 2 of 11 recordings made, the 9 left set aside by the owner |
| [M1.6a](#m16a--the-fake-model-and-the-rivals-bench) | The fake model and the bench, calibrated on Pluto, T3 Code and Zed | Done; stopped by the owner after two runs at rest, the rest moved to M1.6b |
| [M1.6b](#m16b--the-open-rivals-and-the-chats) | The open rivals and the chats measured | Set aside until the owner's go |
| [M1.6c](#m16c--the-vendors-applications-energy-frames-and-the-bar) | The vendors' applications, energy, frames; the bar tightened | Set aside until the owner's go |

## Stage 1 — The ground measured and triaged

Start the new direction from facts: today's figures, and a feature set the owner has chosen. The
server's scope comes from the triage ([ADR-0010](adr/0010-todays-features-triaged.md)).

### M1.1 — Under the method

- **Goal.** One command runs every check, and CI runs it at every push.
- **Delivers.**
  - `scripts/check.sh`: formatting, clippy with warnings as errors, the tests, the documents' links,
    and `cargo deny` for licenses and advisories.
  - `.github/workflows/ci.yml`, running the script in the Nix shell.
  - `docs/ARCHITECTURE.md`: the client, `pluto-core`, `pluto-ffi`, the Android app, and how they
    speak to the fork, as the code is now.
  - The README's top, saying what Pluto is now.
- **Done when.** CI is green on a push. A newcomer can find each part of the code from
  ARCHITECTURE.md.
- **How to test.** `scripts/check.sh`, then the CI run.
- **Measured.** The check's duration, locally and in CI: at most 20 minutes with CI's cache. A run
  without it, the first of a branch or after GitHub drops an unused cache, takes about 30 minutes:
  accepted by the owner on 2026-11-07.
- **By hand.** The owner allows the push, and the CI minutes of a private repository.
- **Done (2026-11-07).** The check passes locally and in CI, timed in
  [`measurements.md`](measurements.md#the-check); the choices are in
  [ADR-0012](adr/0012-the-checks-and-ci.md).

### M1.2 — Today measured

- **Goal.** Every budget of the quality bar has today's figure beside it.
- **Delivers.**
  - On the fixture server (`scripts/fixture-server.sh`), never on the owner's data:
    - the client's memory, split by heaptrack into textures, fonts, the timeline and the rest;
    - its wake-ups at rest: today there are 46 `request_repaint_after` calls;
    - its warm and cold start;
    - the bench on a long thread, extended to 5,000 messages and 15 streams replayed.
  - The fork's resident memory at rest, from outside.
  - What fills the fork's 2.8 GiB database, on a copy: tables, sizes, the kinds of rows.
  - The commands for the HP, which the owner runs there.
  - The figures in `docs/measurements.md`; proposed figures for the budgets left without one.
- **Done when.** Each budget of the quality bar has a measured figure or says why not. The owner
  has validated the new figures.
- **How to test.** The bench and the scripts named in `docs/measurements.md`.
- **Measured.** Everything above.
- **By hand.** The HP's figures, run by the owner.
- **Done (2026-11-07).** Everything but the HP is measured on the desktop, in
  [`measurements.md`](measurements.md), by `scripts/measure.sh` in the new `.#measure` shell
  ([ADR-0014](adr/0014-how-today-is-measured.md)). The fixture builds again: T3 v0.0.45 is pinned
  in the flake. The six budgets without a figure are set, validated by the owner
  ([ADR-0015](adr/0015-the-budgets-set-from-todays-figures.md)). The HP's commands are written;
  the owner runs them, and their figures go to `measurements.md`.

### M1.3 — The triage

- **Goal.** The owner has decided, feature by feature, what the new Pluto keeps.
- **Delivers.**
  - `docs/features.md`: every feature of the client, the server fork and the phone, one row each.
    It is built from `docs/parity/`, the README, `PLUTO.md` and the code. Each row has its place
    in the code and one line on what it does.
  - The owner marks each row keep, cut or rethink.
  - The marks become `docs/specification.md`: what the Rust server and the uwui client must do,
    and the differences of ADR-0003.
- **Done when.** Every row is marked, and the specification is accepted by the owner in an ADR.
- **How to test.** By reading: no row unmarked, and every kept row reachable from the
  specification.
- **Measured.** Nothing.
- **By hand.** The marks, by the owner, in several sittings if needed.
- **Done (2026-11-10).** The 31 families were marked on 2026-11-08
  ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md)); on 2026-11-10 the owner marked the
  358 rows of the 19 rethought families and of the right panel: 195 kept, 106 rethought, 32 cut,
  25 added ([ADR-0022](adr/0022-the-triage-row-by-row.md)). Kept families have no rows: they go to
  the specification whole. The terminal moved to Keep, a mark given by mistake. A script run during
  the closing review found every row not cut named in [`specification.md`](specification.md), and
  every cut row in its section 6. The owner accepted the specification
  ([ADR-0023](adr/0023-the-specification-accepted.md)).

### M1.4 — The cuts

- **Goal.** The egui client and the Android app no longer carry what the triage cut.
- **Delivers.** The code, settings and documents of each row marked Cut in
  [`features.md`](features.md) and listed in the [specification](specification.md#6-what-leaves),
  where the egui client or the Android app has it, in one commit per feature or group. A cut whose
  loss would break daily work before its replacement exists, such as the action editor, is put to
  the owner first. Also: Google's favicon service, and the visual comparison with T3's web client
  (`examples/snapshot.rs`, `scripts/reference/`).
- **Done when.** The checks pass, the client runs on the fixture, and nothing cut remains reachable.
- **How to test.** `scripts/check.sh`, the client run on the fixture, the Android build.
- **Measured.** The client's memory, wake-ups and binary size, before and after.
- **Done (2026-11-10).** Every row of the specification's section 6 is gone from the egui client
  and the Android app where they had it, one commit per group, except three the owner kept until
  the uwui client: the action editor, the evaluation of when-clauses and the custom models' option
  descriptors ([ADR-0024](adr/0024-cuts-kept-in-the-egui-client-until-uwui.md)). The review guides
  (11.16) and Claude's reset claims (18.17) had no client interface; their tool labels stay, to
  name old threads' calls. Google's favicon service left, and an answer's images and icons load by
  themselves only when safe, on the desktop and the phone (`pluto_core::media`). The visual
  comparison with T3's web client and `examples/tmp_shot.rs` left. The client ran on the fixture
  in a headless compositor; the checks pass, the Android build too. Memory, wake-ups and frames
  at rest did not move; the binary lost 1 % ([`measurements.md`](measurements.md#the-cuts-of-m14)).
  The start could not be compared: the machine was loaded, and the client before the cuts was as
  slow as after; it is to be measured again on an idle machine.
- **Closing review (2026-11-10).** It found that every URL in a turn's items named a source, so a
  misled agent could make an image with a secret in its path load by itself: sources are now only
  the pages completed web searches and fetches returned. It also found cuts left half done:
  provider update checks still ran with their switch gone (the owner chose to turn them off: the
  client sends "off" whenever the server's setting is on), other forges and Cursor still showed,
  and the theme commands still had rows. An image the rule refused while loading now waits for a
  click instead of failing. What it found outside M1.4 is in the debt below.

### M1.5 — Agents' sessions recorded

- **Goal.** The drivers of Stage 2 can be tested without a real turn.
- **Decided (2026-11-11).** [ADR-0025](adr/0025-agents-sessions-recorded-and-replayed.md): scripted
  sessions the owner runs; a Rust crate `crates/pluto-recording`; JSON Lines, versioned; Grok as the
  ACP agent; a TypeScript harness kept in Pluto that the fork's test runner runs, guarded in CI by
  a stamp; a disposable home where the owner signs in once.
- **Delivers.**
  - `pluto-record`, which sits between a server and an agent's CLI and writes its raw traffic,
    masked, to a recording: Claude's `stream-json` and Grok's ACP JSON-RPC. `pluto-replay-agent`,
    which plays a recording in the CLI's place.
  - Masking in three layers (a disposable home, stripping by frame type, shapes), and a scan of
    every committed file with detectors of its own, tested.
  - A harness that drives the fork's real adapter: it records, with the real CLI behind
    `pluto-record`, or replays, with `pluto-replay-agent`. Each scenario has hand-written
    assertions, and a reference taken from the live run.
  - The scenarios, for Claude and Grok: a turn, an approval, a question, an interruption, a resume;
    and Grok's background wake.
  - A command per scenario that the owner runs, and the procedure to follow.
  - `scripts/check.sh --fork`, which replays every recording into the fork and writes a stamp;
    CI checks the stamp, scans every recording, and replays each with a client of its own.
- **Done when.** Each recorded file replays into the current server's event mapping and gives its
  live run's reference exactly.
- **How to test.** `scripts/check.sh --fork` on the owner's machine; `scripts/check.sh` in CI.
- **Measured.** Each recording's size, against a budget set from the first ones.
- **By hand.** The real sessions, which only the owner starts: they spend the owner's quota.
- **Built (2026-11-11).** `crates/pluto-recording` holds the recorder, the fake agent, the
  masking and the scan, with 91 tests. Its `fork-harness/` drives the fork's real Claude adapter
  (with the real Claude Agent SDK 0.3.276) and Grok adapter; the self-test records and replays its
  11 synthetic scenarios through them, writing nothing in the fork. `scripts/check.sh --fork`
  runs it and writes the stamp; CI checks the stamp. The procedure is in the
  [crate's README](../crates/pluto-recording/README.md): `scripts/record-session.sh login`, then
  `claude all`, `grok all`, `accept`.
- **Where it stands.** On 2026-11-11 the owner recorded the first sessions. Claude's `turn` and
  Grok's `turn` are in `recordings/` and replay through the fork; they were masked again
  (`pluto-record remask`) once the review of them showed the account's plugins and agents
  (Claude's `initialize` answer) and the subscription's tier (Grok's settings), which the
  stripping now removes. Claude's `approval` was refused by the scan: the random name of the
  project's folder, streamed in pieces, could not be masked; the project's folder is now fixed.
  Grok's `approval` failed for a reason the run did not keep; a failed run now keeps its
  recording, events and error. The other scenarios did not run, since a failure stops the series.
  The owner set the nine scenarios left aside on 2026-11-11, to move to M1.6; they are recorded
  one by one, then `accept`, whenever the owner resumes them. The procedure
  also ran whole as a dry run, stand-ins playing the synthetic recordings
  (`PLUTO_RECORDING_DRY_RUN=1`), and its eleven recordings replayed through the fork. The
  milestone is done when the owner's eleven recordings replay into the fork and give their
  references; their sizes then set the budget.

### M1.6 — The rivals measured

The quality bar holds the rivals' figures, so that Stage 2's server starts from them
([ADR-0017](adr/0017-pluto-measured-against-the-market.md)). A design session on 2026-11-11 split it
in three and decided how ([ADR-0026](adr/0026-the-rivals-measured-how.md)). The editor's rivals are
measured when Stage 7 starts, the phone's with Stage 5. The rivals' licences were read on
2026-11-11: none forbids publishing benchmarks; each is read again before its figures are published.

#### M1.6a — The fake model and the rivals' bench

- **Goal.** One bench measures any desktop application from outside on one workload, calibrated on
  Pluto, T3 Code and Zed.
- **Delivers.**
  - `crates/pluto-fake-model`: Anthropic's Messages, OpenAI's Responses and Ollama's API, with the
    calls clients make at start (model lists, versions); one versioned script of turns (text, a
    tool call mapped to each client's own tool, a Markdown answer), paced; the side requests
    clients make (titles) answered; a journal of each request's bytes by part, and of the prefix it
    repeats from the one before; a check that each tool's result came back.
  - The bench, `scripts/rivals/`: one run of one subject in bubblewrap with a frozen profile, the
    fake model and a headless sway on the GPU; a cgroup per part. Its figures: private, swap and
    GPU memory (PSS and RSS beside), processor time, wake-ups of every thread, start to the first
    frame, frames at rest, the package's size, the profile's growth; a fingerprint per run; the
    median of at least 5 runs.
  - Profiles for Pluto (today's fork and egui client, real Claude Code, Codex and OpenCode CLIs on
    the fake model), T3 Code and Zed, at the vendors' versions of the day.
  - The workload: 15 threads opened then at rest for 10 minutes; 15 threads streaming.
- **Done when.** The fake model's tests pass in `scripts/check.sh`. Claude Code, Codex and OpenCode
  each run a turn with a tool through it, in the bench. Pluto's figures in the bench agree with
  M1.2's within their noise. Pluto, T3 Code and Zed have their figures, with versions and dates,
  in `measurements.md`.
- **How to test.** `cargo test -p pluto-fake-model -p pluto-measure`; the bench's runs, on the
  desktop ([`scripts/rivals/README.md`](../scripts/rivals/README.md)).
- **Measured.** Everything above; nothing of the bar changes yet.
- **Done (2026-11-11), stopped by the owner before its five runs.** The fake model
  (`crates/pluto-fake-model`) and `pluto-measure` are built, reviewed and fixed, with their tests
  in `scripts/check.sh`. Claude Code 2.1.311, Codex 0.164 and OpenCode 2.1.4 each ran a whole turn
  with its tools through the fake model, offline and without an account, outside the bench (no
  journal of those runs is kept); in the bench, Claude Code ran Pluto's and T3 Code's turns, Codex
  Pluto's titles. A first campaign's resource figures were sent back by the closing review (a
  browser counted in T3 Code's, a sandbox that shared the host's process namespace, unequal
  streams); the bench was then fixed: its own process namespace, a cleared environment, only its
  own files, no browser, a GPU check, named busiest threads, a stream measured while all 15
  threads stream. The second campaign measured two calm runs at rest of Pluto and T3 Code 0.1.8,
  one of Zed 1.27.0, when the owner stopped it to move on
  ([`measurements.md`](measurements.md#the-rivals-bench-m16a)). Not done, moved to M1.6b: the
  stream scenario's runs, three more runs at rest of each subject, the frames' own runs, the start
  to the first frame and the profile's growth, and Codex's and OpenCode's turns inside the bench.
  Pluto is not calibrated against M1.2: the states differ, and the client's frames at rest are not
  explained. The fixes made after the closing review were not reviewed again.

M1.6b and M1.6c are set aside until the owner's go
([ADR-0027](adr/0027-the-rivals-measures-set-aside.md)); no session starts them without it.

#### M1.6b — The open rivals and the chats

- **Goal.** Every open rival and chat that takes an endpoint has its figures.
- **Delivers.** Chat Completions in the fake model. OpenCode Desktop, Emdash, Nimbalyst, Jan, Open
  WebUI and LibreChat measured. From M1.6a: Pluto, T3 Code and Zed streaming, and at rest up to
  five runs each; the frames' own runs; the start to the first frame and the profile's growth in
  the bench; frozen profiles; Codex's and OpenCode's turns in it; a streaming thread open in every
  subject's window during the stream scenario; the cause of Pluto's client's frames at rest. The long thread of 5,000 messages, seeded in each application's own
  format where it has one. Real days of 15 threads, several subjects side by side.
- **Done when.** Each has its figures, or a written reason.

#### M1.6c — The vendors' applications, energy, frames, and the bar

- **Goal.** The bar holds the stricter of its budgets and the best rivals' figures.
- **Delivers.** Claude's desktop application in its 3P mode and ChatGPT's through Codex, on the fake
  model, labelled so; ChatGPT's chat, Msty Studio, Warp and LM Studio at rest or with a written
  reason. The energy sampler for the owner's NixOS, which the owner builds; energy at rest and over
  15 streams. Each frame's production time and the frames at rest. The HP's command, which the
  owner runs. The ROADMAP's budgets tightened, in an ADR.
- **Done when.** Every agents' application and chat of the bar has its figures, or a written
  reason; the ROADMAP's budgets are the stricter of the two.
- **By hand.** The owner builds the energy sampler and runs the HP's command.

## Ideas

- The fork exists on one disk only: its branch `pluto` is pushed nowhere. A private copy would
  protect it; the owner's call.
- Two weeks of local, private counts of which features the owner uses, to inform the triage. Kept
  on the machine, never sent; ask before building.
- From the 2026-10-27/28 backlog, set aside by the owner:
  - OpenDesign's catalog of design systems;
  - Ollama's model management (now in Stage 3);
  - a Nix build without the web client.

## Known debt

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
  protocol replaces this code in Stage 2.
- AccessKit's D-Bus thread (`blocking-1`, zbus) wakes the client twice a second at rest, the
  largest of its wake-ups (M1.2). It comes with eframe's accessibility, which the screen reader
  needs. Followed by Stage 6: the uwui client leaves eframe.
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

  Followed by Stage 2 for the server, whose Rust server replaces the fork, and by Stage 6 for the
  client, which the uwui client replaces. The egui client is frozen and is not tuned meanwhile.
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
  an ordinary key, and a `password=` split across streamed deltas. Followed by Stage 2's first
  driver milestone.
- Pluto's interface shows a native subagent's effort as the parent thread's, not the effort its
  definition sets, which its requests to the model carry. It is the fork's; the fork changes only
  on the owner's word.
- Before M1.5's harness kept its caches outside the fork, two test runs of 2026-11-10 updated the
  fork's own `node_modules/.vite/vitest/<hash>/results.json` and `.vite-temp`, which git ignores.
  Left as they are: they are caches, and the owner's own runs write them too.
- The rivals' bench found in today's Pluto at rest after 15 threads
  ([`measurements.md`](measurements.md#the-rivals-bench-m16a)): the egui client wakes 31 times a
  second (its main thread and its clipboard thread), and drew 6 frames every 1.25 s in the first
  campaign, its cause not found; every Claude Code session the fork started stays alive, about
  1.45 GiB and 325 wake-ups a second for 15; the fork's titles go through Codex, about 54 KB sent
  per title request. The fork leaves in Stage 2; the client is frozen except fixes.
- The rivals' bench counts frames but cannot judge them on time, and has no energy yet
  ([ADR-0026](adr/0026-the-rivals-measured-how.md)). Its scripts are not linted: `shellcheck` is
  not in the Nix shell, as for M1.5's.
- CI does not run the Android app's Kotlin tests nor build its APK; `scripts/check.sh --apk` does,
  locally ([ADR-0012](adr/0012-the-checks-and-ci.md)). Followed by Stage 1's audit.

Finished stages move to `docs/history.md`, created when the first one ends, in a few lines
each, so that this file holds the current stage only, under about 2,000 words.
