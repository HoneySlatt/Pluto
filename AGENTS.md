# AGENTS.md

Rules for AI agents working in this repository. Read this file fully at the start of every session.

## Project

Pluto is a harness of its own: one place to work with every model and every coding agent (Claude
Code, Codex, Copilot, OpenCode, Kimi Code, Mistral Vibe, Z Code, the ACP agents, DeepSeek, local models through
Ollama), for code, chat, design and orchestration, and later a personal assistant. It is native,
frugal and private. It is in Rust, for Linux first, under MIT (new code MIT OR Apache-2.0).

Its direction changed on 2026-11-06 ([ADR-0003](docs/adr/0003-what-pluto-is-to-become.md)), then
on 2026-11-15: Pluto becomes a harness ([ADR-0030](docs/adr/0030-pluto-becomes-a-harness.md) to
[ADR-0040](docs/adr/0040-out-of-the-plan.md)). Its state today:

- **New foundation**: new crates from Stage 2, beside the frozen code below
  ([ADR-0038](docs/adr/0038-a-new-foundation-beside-the-fork.md)).
- **Desktop client**: on egui (`src/`, about 110,000 lines), frozen except fixes. The client on
  uwui, the owner's UI framework, replaces it at Stage 5.
- **Shared core**: the connection, model and timeline, shared with the phone (`crates/pluto-core`).
- **Android app**: Kotlin and Compose over the core through UniFFI (`android/`, `crates/pluto-ffi`);
  on Pluto's protocol at Stage 5.
- **Server**: a frozen TypeScript fork of T3 Code's server, `pluto-server`, at
  `~/Projects/pluto-server`, branch `pluto` (see its `PLUTO.md`). Pluto's own server replaces it at
  Stage 5.

Agents develop it; the owner sets its direction and validates its decisions. Anyone must be able to
understand it from the repository alone. Documentation is part of every deliverable.

## Read before working

- `docs/PLAN.md`, its *Current* section and the current milestone: what to do now.
- When planning, or when the task touches a decision: `docs/ROADMAP.md` (direction, stages, the
  quality bar), `docs/ARCHITECTURE.md` (the code's parts and flows), and `docs/adr/`.
- `docs/uwui-needs.md` before any change to what the client asks of its UI framework.

You remember nothing from earlier sessions. What is not written in these files is lost.

## Scope

- Work on the current milestone of `docs/PLAN.md`, or on what the owner asks. A request that changes
  the plan is written into the PLAN.
- Do not start later stages; build for them only what an accepted ADR asks.
- Add no feature to the egui client: it is frozen ([ADR-0010](docs/adr/0010-todays-features-triaged.md)).
- Do not replace the client's framework with a web view.
- When experience contradicts the roadmap, propose a change; once accepted, record it in an ADR.

## Commands

```sh
nix develop                   # the toolchain, the checks' tools, eframe's libraries; `.#mobile` for Android
scripts/check.sh              # every check CI runs (its head lists them); `--apk` builds Android too;
                              # `--fork` replays the recordings through the fork, on the owner's machine
scripts/fixture-server.sh     # a disposable server on :3999 with synthetic threads
cargo run --release --example bench -- http://127.0.0.1:3999 /tmp/t3-egui-fixture/bearer
cargo build --release -j2     # the binary the owner's shortcut starts; more jobs get OOM-killed
```

The README's *Development* section has the rest: the measurements, the icon generators, and the
agents' recordings (`scripts/record-session.sh`, the owner's alone: it runs real agents).

Never, on your own:
- run a real provider turn: sending a message starts a session on the owner's accounts and spends
  their quota. Automated checks use fixtures, fakes and recorded sessions. One exception: Pluto's
  bench may run on local models through Ollama, by hand during a milestone, never in CI
  ([ADR-0037](docs/adr/0037-how-the-harness-is-judged.md));
- touch the owner's data (`~/.pluto`, `~/.t3`, the agents' own homes): use disposable fixtures,
  and copies for measurements;
- modify `~/Projects/pluto-server`, SlimeEngine or uwui, unless the owner asks.

## Definition of done

- `scripts/check.sh` passes.
- The program was run and behaved as expected.
- New behaviour is tested where a test is practical.
- The budgets the milestone touches are measured and met; the figures go to
  `docs/measurements.md`.
- `docs/PLAN.md` gives the new status.
- Every document the change affects is updated and reread as a newcomer would.
- Every decision a newcomer would question is an ADR.

Never report as working what was not verified. Say exactly what was not, and why. Never weaken a
check to make it pass (a skipped test, a raised budget, a silenced lint) without the owner's go.

## Architecture rules

- The protocol is Pluto's own, described once in a shared crate
  ([ADR-0005](docs/adr/0005-plutos-own-typed-protocol.md)). New code never speaks T3's protocol;
  only the egui client and the Android app do, until the end of Stage 5.
- Each agent session runs in its own `pluto-agent` process; its append-only log is the session's
  truth, the context a view of it ([ADR-0033](docs/adr/0033-one-pluto-agent-process-per-session.md)).
  It runs in its own unit, never as a child of the server, the command line or a lead agent, and
  an agent approves only what its brief grants ([ADR-0043](docs/adr/0043-each-session-in-its-own-unit.md)).
- Pluto's own code has no Node. The one exception is Claude Code's mod, in TypeScript
  ([ADR-0031](docs/adr/0031-three-kinds-of-engine.md)).
- The server and the protocol crate never depend on a UI crate. `pluto-core` never depends on egui.
- The interface's thread never waits on the network or a disk.
- Nothing runs, polls or redraws while there is nothing to do: no timer that wakes for nothing.
- Engines follow [ADR-0031](docs/adr/0031-three-kinds-of-engine.md) and
  [ADR-0006](docs/adr/0006-agents-drivers.md): tested on recorded sessions or fakes. A component of
  Pluto's loop stays only while the bench shows its gain
  ([ADR-0037](docs/adr/0037-how-the-harness-is-judged.md)).

## Rules the code depends on

- Never discover, read, log or persist other programs' credentials. Secrets are entered by the owner
  and live in the system's keyring, or in memory until it exists
  ([ADR-0009](docs/adr/0009-secrets-privacy-and-the-sandbox.md)).
- Nothing leaves the machine of Pluto's own accord: no telemetry, no update check without consent.
  One exception: an answer's images and link icons load by themselves when safe, from the hosts of
  its cited sources or tool results, never through a third party
  ([ADR-0022](docs/adr/0022-the-triage-row-by-row.md)).
- Every format the server or client writes is versioned and migrated; writes are atomic; a
  migration backs up first ([ADR-0008](docs/adr/0008-storage-and-the-history-moved.md)).
- Honour each provider's terms: drive their official CLIs, or a sign-in the provider offers to
  third-party applications ([ADR-0020](docs/adr/0020-the-chat.md)); never scrape their web services.
- When reading T3 Code's original sources, extract them from the installed package's source maps;
  never vendor them here. Code taken from T3 (MIT) keeps its notice in `NOTICE.md`.

## Code rules

- No abstraction without a second concrete use, or a benefit an accepted ADR names.
- No reimplementation of the standard library without a measurement.
- A new dependency needs a reason, an alternative weighed and a license checked; ask the owner.
- Every `unsafe` block has a `// SAFETY:` comment.
- Code reads plainly to someone who has never seen it: names that say what things are, one job per
  function, the obvious solution over the clever one.
- Comments say why, not what. No commented-out code.
- A bug is fixed at its cause, with the test that failed first; look for the same bug elsewhere.
- Edge cases are thought through and tested: empty, huge, malformed, concurrent, interrupted,
  offline, a server that drops.
- Search for an existing helper before writing one. One way to do a thing.
- Delete dead code and unused parameters.
- Report drift from `docs/ARCHITECTURE.md` or an ADR; do not work around it.

## Documentation

- It is written for a newcomer: plain words, short sentences, the essential first, as short as
  complete.
- It describes the code as it is now. Wrong documentation is worse than none.
- No reference to conversations the reader cannot see. Coined terms go in `docs/glossary.md`.
- Limits and debt are stated plainly.
- Each fact has one home; other documents link to it:
  - the code's documentation: contracts, limits, figures;
  - `docs/ARCHITECTURE.md`: flows across parts;
  - ADRs: why a decision was taken, what was rejected; only their status changes later;
  - `docs/PLAN.md`: status and known debt;
  - `CHANGELOG.md`: what changed for users.
- A change that leaves a document wrong is not finished.

## Commits

- Commit and push whenever the work needs it, without asking (the owner's decision of
  2026-11-07), once `scripts/check.sh` passes; then follow CI to the end.
- One logical change per commit, in English, its subject in the imperative.
- Never commit code that fails the checks. A check that fails on the main branch is fixed before
  any new work.

## When to stop and ask

Before a decision no ADR covers, a new dependency, a request against the roadmap or an ADR, a plan
that turns out wrong, or deleting or rewriting a large part of the code. Give the options, their
trade-offs in plain terms and a recommendation for the long term, one decision at a time.

## Reporting

A stage ends with a stage audit, and a release is preceded by a release audit; their findings go to
`docs/audits/`. Ideas outside the current work go to the *Ideas* of `docs/PLAN.md`.

At the end of each milestone, after a reviewing agent on the strongest model has checked it against
this file, the ADRs and the quality bar, report: what was done; the choices, why, and the
alternatives rejected; what to read first; the figures measured; shortcuts and debt; the next step,
on the owner's go. The next milestone starts in a new session, unless the owner says to continue.

## Working with several agents

- One milestone per agent, as `docs/PLAN.md` assigns it.
- Shared files (`docs/PLAN.md`, `docs/ARCHITECTURE.md`, the build manifests): one agent at a time.
- Commit your files by their paths, never with `git add -A`.
- Delegate broad searches to subagents; keep their conclusions, not the files they read.

## Language

Everything in the repository is in English.
