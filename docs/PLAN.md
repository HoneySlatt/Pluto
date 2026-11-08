# Plan

What is being worked on now: the milestones of the current stage, their status, how each one is
checked, and the known debt. Direction beyond this stage is in [`ROADMAP.md`](ROADMAP.md); decisions
are in [`adr/`](adr/README.md). A *stage* is a step of the roadmap; a *milestone* is a small,
verifiable step of a stage.

Every milestone ends with the definition of done in [`AGENTS.md`](../AGENTS.md#definition-of-done).

## Current

- DeepSeek Harness joined today's Pluto as a `deepseekHarness` provider of the fork, through its
  official local Web protocol ([ADR-0021](adr/0021-deepseek-harness-in-todays-pluto.md)). It
  streams text, reasoning and tool activity, answers approvals and questions, supports Stop and
  resume, and maps Pluto's four runtime modes onto Harness permission presets. Tests use fake
  peers and a recorded 0.2.1-rc.3 wire capture; the real binary was run against a fake local
  model, and no real account turn was made. Sign-in and a paid turn remain the owner's
  validation. Limits, stated in the provider guide: no native forks, rollback, plan mode,
  application text or token usage.
- On 2026-11-06 Pluto came under the owner's method, and changed direction in a design session with
  the owner ([ADR-0003](adr/0003-what-pluto-is-to-become.md) to
  [ADR-0010](adr/0010-todays-features-triaged.md)), and on 2026-11-07 chose a harness of its own
  ([ADR-0011](adr/0011-plutos-own-harness-minimal-and-measured.md)).
- Pluto is no longer a port of T3 Code. `docs/release-backlog.md` is abandoned, and
  `docs/parity/` stays as an inventory.
- Stage 1: [M1.1](#m11--under-the-method) and [M1.2](#m12--today-measured) are done, the HP's
  figures to come by the owner's hand. Work happens on `main` since 2026-11-07.
- On 2026-11-08 an interview with the owner set Pluto against the market, the market's features,
  the triage by family, a code editor in Pluto and the chat
  ([ADR-0017](adr/0017-pluto-measured-against-the-market.md) to [ADR-0020](adr/0020-the-chat.md)).
  [M1.3](#m13--the-triage) is under way: the families are marked, their rows come next. Then
  [M1.6](#m16--the-rivals-measured), beside M1.4 and M1.5.
- What Pluto asks of uwui is in [`uwui-needs.md`](uwui-needs.md), for SlimeEngine's M6d.9a, M6d.9b
  and M6d.10.

## Status

| Milestone | Goal | Status |
|---|---|---|
| [M1.1](#m11--under-the-method) | Every check in one command and in CI; the code's architecture written down | Done |
| [M1.2](#m12--today-measured) | Today's client, server and HP measured; the fork's database understood | Done; the HP's figures to come |
| [M1.3](#m13--the-triage) | The owner has triaged every feature; the specification exists | Under way: families marked, rows to come |
| [M1.4](#m14--the-cuts) | The egui client and the Android app lose what the triage cuts | Planned |
| [M1.5](#m15--agents-sessions-recorded) | Recorded agent sessions that drivers' tests can replay | Planned |
| [M1.6](#m16--the-rivals-measured) | The rivals measured on one workload; the bar set from them | Planned |

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
- **Where it stands (2026-11-08).** The 31 families are listed and marked in
  [`features.md`](features.md) ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md)): 11
  kept, 20 rethought with the phone, none cut. The features added by the interview are listed
  there too. Next: the rows of each rethought family, each with its problem, the rival that does it
  best, and core or optional; the owner's reasons for the families rethought against the
  recommendation (2, 7, 15, 28) and the direction of design projects (25); Bitbucket and T3's
  environment themes. Then `docs/specification.md`.

### M1.4 — The cuts

- **Goal.** The egui client and the Android app no longer carry what the triage cut.
- **Delivers.** The code, settings and documents of each cut feature removed, in one commit per
  feature or group.
- **Done when.** The checks pass, the client runs on the fixture, and nothing cut remains reachable.
- **How to test.** `scripts/check.sh`, the snapshot example, the Android build.
- **Measured.** The client's memory, wake-ups and binary size, before and after.

### M1.5 — Agents' sessions recorded

- **Goal.** The drivers of Stage 2 can be tested without a real turn.
- **Delivers.**
  - A recorder that saves an agent's raw protocol traffic to a file: Claude's `stream-json`, and
    one ACP agent's JSON-RPC.
  - A first set of sessions recorded by the owner, by hand, on a disposable project: a turn, an
    approval, a question, an interruption, a resume.
  - Secrets are masked before a file is saved.
- **Done when.** Each recorded file replays into the current server's event mapping without a
  difference.
- **How to test.** The replay tests, in `scripts/check.sh`.
- **Measured.** Nothing.
- **By hand.** The real sessions, which only the owner starts: they spend the owner's quota.

### M1.6 — The rivals measured

- **Goal.** The quality bar holds the rivals' figures, so that Stage 2's server starts from them
  ([ADR-0017](adr/0017-pluto-measured-against-the-market.md)).
- **Delivers.**
  - A scripted fake model service that streams one workload, a long thread and 15 threads, to any
    application that takes a custom endpoint, Pluto included; it counts the tokens it receives.
  - A disposable, isolated profile for each rival, its network cut unless the measure needs it.
  - The agents' applications and the chats of the quality bar measured on the desktop and the HP:
    memory and processor at rest, wake-ups, start, frames, energy, the whole system over a day of
    15 threads. Each with its version and date in `measurements.md`. ChatGPT's desktop application
    runs in the owner's Windows machine. The applications that take no custom endpoint, Claude's
    and ChatGPT's, are measured at rest and over a session the owner runs by hand, with the owner's
    go per account. The editor's rivals are measured when Stage 7 starts, the phone's with Stage 5.
  - The bar's figures tightened where a rival's figure and its margin are stricter.
- **Done when.** Every agents' application and chat of the bar has its figures, or a written
  reason; the ROADMAP's budgets are the stricter of the two.
- **How to test.** The fake model's own tests, in `scripts/check.sh`; the measures by
  `scripts/measure.sh`.
- **Measured.** Everything above.
- **By hand.** The owner's one go for downloading the list of rivals outside Nix, and a go per
  account for the Claude and ChatGPT applications. Each rival's licence checked before any figure
  is published.

## Ideas

- Two weeks of local, private counts of which features the owner uses, to inform the triage. Kept
  on the machine, never sent; ask before building.
- From the 2026-10-27/28 backlog, set aside by the owner:
  - OpenDesign's catalog of design systems;
  - Ollama's model management (now in Stage 3);
  - a Nix build without the web client.

## Known debt

- The egui client stays at T3 v0.1.0 parity with the gaps listed in `docs/parity/`. Those gaps are
  no longer debt: the triage decides what of them matters.
- The Z Code driver has never run a real turn.
- `docs/release-backlog.md` describes a plan that no longer exists. It stays for its record of what
  was done, and is removed once the triage has used it.
- Visual checks against the reference are manual (`examples/snapshot.rs`). M1.1 put it off: the
  reference is T3's web client, whose parity is abandoned. M1.3 decides whether the comparison
  stays; the uwui client's visual checks replace it.
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
- The desktop fetches favicons from Google's favicon service and loads messages' `https:` images
  of its own accord, against "nothing leaves the machine of Pluto's own accord". By the owner's
  decision of 2026-11-07, the triage (M1.3) decides it and the cuts (M1.4) apply it.
- CI does not run the Android app's Kotlin tests nor build its APK; `scripts/check.sh --apk` does,
  locally ([ADR-0012](adr/0012-the-checks-and-ci.md)). Followed by Stage 1's audit.
- `examples/tmp_shot.rs` calls itself throwaway but stays in the tree. Followed by M1.4.

Finished stages move to `docs/history.md`, created when the first one ends, in a few lines
each, so that this file holds the current stage only, under about 2,000 words.
