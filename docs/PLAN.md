# Plan

What is being worked on now: the milestones of the current stage, their status, how each one is
checked. Direction beyond this stage is in [`ROADMAP.md`](ROADMAP.md); decisions are in
[`adr/`](adr/README.md); the stages done are in [`history.md`](history.md); the known debt is in
[`debt.md`](debt.md). A *stage* is a step of the roadmap; a *milestone* is a small, verifiable step
of a stage.

Every milestone ends with the definition of done in [`AGENTS.md`](../AGENTS.md#definition-of-done).

## Current

- **On 2026-11-15 Pluto became a harness**: [ADR-0030](adr/0030-pluto-becomes-a-harness.md) and
  the ADRs after it; the new stages are in the [ROADMAP](ROADMAP.md#4-stages).
- **Stage 1's milestones are done or moved** ([`history.md`](history.md)). Stage 1 closes with its
  stage audit, which comes next, in a session of its own, following the method's `audits.md`; the
  fate of the unpushed branch `m16b2b` is one of its questions. Then
  [M2.0](#m20--the-new-foundations-architecture).
- The egui client, the Android app and the fork run unchanged until the end of Stage 5
  ([ADR-0038](adr/0038-a-new-foundation-beside-the-fork.md)). DeepSeek Harness is a provider of
  the fork ([ADR-0021](adr/0021-deepseek-harness-in-todays-pluto.md)).
- What Pluto asks of uwui is in [`uwui-needs.md`](uwui-needs.md). The owner expects uwui to be ready
  before Stage 3.
- **Urgent, found on 2026-11-16**, each costlier the longer it waits:
  - the fork's branch `pluto` is pushed nowhere, and the owner's NixOS builds it from a local path:
    a private copy, on the owner's go;
  - the owner's history in `~/.pluto` needs a backup before Stage 5's import: the owner's to
    confirm;
  - the method changed on 2026-11-16: architecture is part of the bar (change and failure
    scenarios, invariants, ownership, recovery, each ADR's validation and reconsideration).
    Stage 1's audit reviews Pluto against it; M2.0 applies it;
  - the license: the root crate, `pluto-core`, `pluto-ffi` and `uniffi-bindgen` declare MIT, the
    four tool crates MIT OR Apache-2.0, and the repository holds only T3's MIT text, under T3's
    name; no Apache-2.0 text, no copyright line of Pluto's own. Code copied from Codex's sandbox,
    under Apache-2.0, could not be offered under MIT (ADR-0035 plans a notice only). Fixed before
    any outside contribution or the first line copied from Codex, with a DCO or a CLA; CI also
    runs on pull requests by then.
- **The [audit of 2026-11-17](audits/2026-11-17-architecture-and-contribution.md)**, on the
  owner's demand, examined the architecture, contribution and what to decide early. The owner took
  its first decision: each session in its own unit, and an agent approves only what its brief
  grants ([ADR-0043](adr/0043-each-session-in-its-own-unit.md)). Its other findings are planned
  below, in Stage 1's audit, M2.0 and M2.1.
- **Stage 1's audit also asks**, from a review of 2026-11-16 and the audit of 2026-11-17:
  - `AGENTS.md` still says "no abstraction without a second concrete use"; the method now also
    accepts a boundary around an unstable dependency, a protected invariant or a test. Align, on
    the owner's go, before M2.0, whose injected clock and platform boundary the old rule forbids;
  - the contradictions the audit lists: PLAN's version on every message against ADR-0005's at
    connection; the ROADMAP's protocol at Stage 5 against Stage 3's channel;
  - the chains of amendments consolidated, the foundation's topics first (0005 and 0033, 0009 and
    0035, 0006 and 0031), each with a validation section;
  - the quality bar given its change and failure scenarios, from the audit's table;
  - ADR-0020 says to check shortcuts with Umbriel; the compositor is now Niri.

## Status

| Milestone | Goal | Status |
|---|---|---|
| Stage 1's audit | The whole project examined at Stage 1's end; its findings in `docs/audits/` | Planned |
| [M2.0](#m20--the-new-foundations-architecture) | The new foundation's parts, boundaries and ways to extend it, written and checked before its first crate | Planned |
| [M2.1](#m21--the-session-logs-format) | One versioned format holds any session; the context is a view of it | Planned |
| [M2.2](#m22--the-benchs-tasks-and-budget) | The owner has chosen what the bench runs and what it may spend | Planned |
| [M2.3](#m23--the-benchs-runner) | One command runs a task with an engine and a model, sandboxed, and grades it | Planned |
| [M2.4](#m24--a-subset-of-terminal-bench) | A public reference through the same runner | Planned |
| [M2.5](#m25--the-vendors-harnesses-measured) | The figures Pluto's loop must reach | Planned |

## Stage 2 — The bench and the session log

Pluto measures any harness on the owner's own work and on a public benchmark before it builds its
own loop ([ADR-0037](adr/0037-how-the-harness-is-judged.md)), and fixes the format every session
writes from Stage 3 on ([ADR-0033](adr/0033-one-pluto-agent-process-per-session.md)).

### M2.0 — The new foundation's architecture

- **Goal.** The new foundation's parts, their boundaries and the ways to extend it are written and
  checked before its first crate, so that the harness can change easily and newcomers can
  contribute (the owner's request of 2026-11-16). Taken already: how sessions are supervised
  ([ADR-0043](adr/0043-each-session-in-its-own-unit.md)).
- **Decided first.** A design session, without code: how Codex, goose, Zed and OpenCode lay out
  their crates and extension points; the options; a challenger; the decisions put to the owner one
  at a time, then ADRs. A survey of 2026-11-16 of what harnesses had to redo late, its primary
  sources in the [audit of 2026-11-17](audits/2026-11-17-architecture-and-contribution.md#sources),
  names what it must settle:
  - sessions survive a restart or an update of the server: each in its own unit, decided by
    ADR-0043;
  - the log: one writer per session, retired event types readable forever, no silent deletion;
  - identifiers unique across machines, independent of the machine and the environment;
  - the protocol: a version and capabilities on every message, unknown entries kept and shown but
    an unknown critical one refused (the writer marks each entry's class), schemas
    generated for Kotlin and the mod, experiments behind an opt-in; an authenticated local server
    from its first commit;
  - who owns each state, how each task is cancelled and stopped, every queue and cache bounded
    with its behaviour at the limit; typed errors with stable codes;
  - bounded, structured diagnostics with no secret; costs recorded once in the log, with cache,
    price tiers and billing route; a clock and randomness injected for tests; contract tests
    against each CLI version Pluto drives;
  - platform code (paths, shells, sandbox backends) behind one boundary, the platform-free crates
    checked on Windows and macOS in CI; the durable write and the log's one-writer lock designed
    and tested for Windows and macOS too, since Pluto reaches them at Stage 8
    ([ADR-0044](adr/0044-five-platforms-at-going-public.md));
  - permissions as typed actions, never parsed shell strings; a trust gate before a project's
    files are read or run; the sandbox denying the network, the session bus and the keyring;
    content's provenance kept, tool output never taken as instructions; every file Pluto loads
    pinned by digest, a repository's files inactive until trust, layers that restrict and never
    grant, hooks from the owner alone with defined outcomes, skills in the open format, an
    inventory before anything is enabled, and Pluto's own features through its contracts
    ([ADR-0046](adr/0046-what-pluto-loads-from-files.md));
  - Pluto's own Nix packages and module, its engines' CLIs pinned in its flake, and the owner's
    rules and skills read from a path its Nix module gives, never guessed among the agents' own
    copies;
  - a review of 2026-11-16 adds what that survey left out:
    - the execution model, decided once: an async runtime (tokio, which `rmcp` and iroh use) or
      threads. Formats, the protocol and the loop's logic do no I/O, so that they replay, fuzz and
      port; I/O stays at the edges;
    - authorization, not only authentication: tokens scoped by device, client and child agent, as
      the fork's step C did late; a token never passed in an environment variable (the fork's ACP,
      Pi and Kimi children keep theirs there, readable by the owner's other processes); the server
      bound to loopback unless told otherwise (the fork listens on every address);
    - room in the log, from M2.1, for what comes later: checkpoints (Stage 3), the digest of the
      profile, prompt and tools at each turn (for the bench), sessions with no repository (the
      chat, the assistant), segments that can be compressed (the storage budget);
    - a corpus of every past version of the log, the protocol and the settings, read by
      `scripts/check.sh`; every reader of outside input fuzzed (the log, the protocol, the CLIs'
      output);
    - the data's folders, with the public name: XDG's config, data, state and cache, or one folder;
      one server per data folder, locked;
    - the workspace's rules: shared lints (no `unwrap` outside tests, `unsafe` forbidden where
      unused), dependency versions shared, public items documented, a toolchain file so that cargo
      builds without Nix;
    - the frozen client out of the repository's root, so that the root builds the new foundation.
  - the audit of 2026-11-17 adds, in this order after the workspace's split:
    - the data folders and the public name, decided before M2.1 fixes a path; the audit
      recommends the logs in XDG's data, the index in its state, derived and rebuilt from the
      logs, the cache apart, and one root a test profile can move;
    - no third-party type in a persisted format or the protocol, checked with `cargo deny`'s
      `wrappers` beside the direction check; error codes in reserved ranges, which the interface
      translates later;
    - one durable-write function (temporary file, `fsync`, rename, `fsync` of the folder), the
      other ways refused by clippy in the new crates ([`debt.md`](debt.md) has the client's seven);
    - a one-page trust map: a tool process per call; the sandbox without the session bus,
      keyring and systemd sockets; where tool code can and cannot reach an engine's credentials;
    - the command line's channel to `pluto-agent` as the protocol crate's agent part, versioned;
      Stage 5 keeps only the network's transport and encoding;
    - the license's fix and a DCO, `SECURITY.md` with GitHub's private reporting, CI on pull
      requests; bidirectional text asked of uwui in [`uwui-needs.md`](uwui-needs.md).
- **Delivers.**
  - In [`ARCHITECTURE.md`](ARCHITECTURE.md), the target: the crates of Stages 2 to 5 by name, what
    each holds, the direction of their dependencies, and each one's public surface.
  - The seams as contracts: the session log's schema, the provider client, the engine, the tool;
    a recipe to add a provider, a tool or an engine, each in one place.
  - A check in `scripts/check.sh` that refuses a dependency against the direction.
  - The frozen egui client and its crates in a workspace of their own, so that the new foundation
    builds and tests without them.
  - `CONTRIBUTING.md`: the environment, the commands, where to start. `SECURITY.md`: how to
    report a vulnerability privately.
- **Done when.** The ADRs are accepted. The dependency check fails on a test crate that breaks the
  direction and passes on `main`. The new foundation's checks run without building eframe.
- **How to test.** `scripts/check.sh`; a newcomer test: a fresh agent, given only the README,
  `CONTRIBUTING.md` and `ARCHITECTURE.md`, finds where a provider, a tool and an engine are added.
- **Measured.** The check's duration, locally and in CI, before and after the split.
- **By hand.** The owner's decisions.

### M2.1 — The session log's format

- **Goal.** One versioned format holds any session from any engine, and the context a model
  receives is a view of it.
- **Decided first.** The encoding, the entries, the provenance of content, and how a torn write is
  repaired: options put to the owner, then an ADR. The entries must hold, from the first line
  ([audit of 2026-11-17](audits/2026-11-17-architecture-and-contribution.md)): an intent before
  each effect Pluto runs and its outcome after, paired by the call's id, an interruption appended
  on resume and nothing run again by itself; the id a client chose, and a gapless sequence per
  session; each entry's class, so that an unknown critical one is refused; the parent session and
  its time limit, and who gave each approval under which grant (ADR-0043); the digest of each
  method file read; an engine's unknown event kept raw.
- **Delivers.**
  - `crates/pluto-log`: the entries (messages, tool calls and results, usage, context edits,
    fetched pages, provenance), versioned from the first line; an append-only writer whose
    appends are atomic; a torn last entry repaired on opening; a reader; the context view.
  - A converter from Stage 1's recordings (Claude Code's `stream-json`, Grok's ACP) into logs.
- **Done when.** Each recording, real or synthetic, converts into a log whose view gives the
  scenario's reference exactly. The tests cover an empty log, a huge one, malformed and torn
  entries, a second writer refused, a newer version refused with a plain message, entries from a
  future version kept and shown, and a session killed between an intent and its outcome.
- **How to test.** `cargo test -p pluto-log`, in `scripts/check.sh`.
- **Measured.** An append's time, its `fsync` included, on the desktop and on the HP's disk; a
  log's size per turn on the recordings, against the storage budget of 25 MiB a day
  ([ROADMAP](ROADMAP.md#quality-bar)).

### M2.2 — The bench's tasks and budget

- **Goal.** The owner has chosen what the bench runs and what it may spend.
- **Delivers.**
  - A survey of the owner's saved briefs and their hidden checks: how many, from which projects,
    which kinds of work, which still replay at their base commit, how long their checks run.
  - A proposal: the task set; the subset of Terminal-Bench and its size against the noise (how
    many tasks resolve which difference); runs per task; the models; the monthly budget; which
    account each frontier run uses, under each provider's terms.
  - What of the briefs may sit in this public repository: a manifest of ids and hashes with the
    briefs kept on the machine, tasks from public repositories only, or another way.
  - The decisions put to the owner one at a time, then an ADR.
- **Done when.** The ADR is accepted.
- **How to test.** By reading, and by a check of each chosen task: its hidden checks fail at its
  base commit and pass on the commit that closed it.
- **Measured.** Each candidate set's resolution, estimated from its size; a run's cost per model,
  estimated at list prices.
- **By hand.** The owner's choices.

### M2.3 — The bench's runner

- **Goal.** One command runs one task with one engine and one model, in a sandbox, and grades it.
- **Delivers.**
  - `pluto-bench`: the task's workspace at its base commit, in bubblewrap, with no network but the
    model's endpoint: on the fake model, an empty network namespace with the model inside; on a
    local model, one forwarded port; on a frontier model (M2.5), an allowlist of hosts that never
    decrypts TLS ([ADR-0031](adr/0031-three-kinds-of-engine.md)). Each run in its own unit, which
    dies with the runner ([ADR-0043](adr/0043-each-session-in-its-own-unit.md)). Results written
    through M2.0's durable-write function.
  - The engines run headless: Claude Code (`claude -p --output-format stream-json`), Codex
    (`codex exec --json`) and OpenCode (`opencode run`), each also on local models through Ollama.
    Pluto's own loop joins in Stage 3.
  - The hidden checks run after the agent stops, outside its sandbox, out of its reach.
  - A versioned results file per run: the grade, tokens in, cached and out, the cost at list
    prices, the time, turns and tool calls, and a fingerprint of the environment (engine, model,
    versions, machine, load). Repeats summed with their confidence intervals.
  - Each run's transcript kept; Claude Code's converted into a log by M2.1's converter.
- **Done when.** In CI, each engine runs a task end to end on the fake model; a passing and a
  failing task are graded right; the same run twice gives the same grade; the fingerprint changes
  with the engine's version.
- **How to test.** `cargo test -p pluto-bench`, and the fake model's runs, in `scripts/check.sh`.
- **Measured.** The runner's own cost per run, in time and memory, so that it biases no figure.

### M2.4 — A subset of Terminal-Bench

- **Goal.** A public reference, through the same runner and results.
- **Delivers.** The subset chosen in M2.2, run through Terminal-Bench's own harness, Harbor, or by
  the runner directly; its results in M2.3's format. Harbor and the containers its tasks need are
  new dependencies, asked of the owner first.
- **Done when.** One task of the subset runs in CI on the fake model, and a run on a local model
  gives Harbor's own grade.
- **How to test.** The runner's tests; one run by hand on a local model.
- **Measured.** A run of the subset's time and resources on the desktop.

### M2.5 — The vendors' harnesses measured

- **Goal.** The figures Pluto's loop must reach, on frontier, cheap and local models.
- **Delivers.**
  - Campaigns of Claude Code, Codex and OpenCode on the bench and the subset, on M2.2's models.
    Agents run the local models through Ollama, by hand. The frontier models are the owner's runs,
    within the month's budget.
  - For the owner's method, the efforts of its roles compared on their own briefs, for its table of
    models.
  - The figures, with their intervals, in [`measurements.md`](measurements.md), and the ROADMAP's
    harness rows given their reference.
- **Done when.** Each engine and model of M2.2's list has its figures, or a written reason.
- **How to test.** The results files and their summary.
- **Measured.** Score, tokens, cache hits, cost and time per task; the machine's load checked
  before and during each timed run, a disturbed run marked so.
- **By hand.** The frontier runs, the owner's.

## Ideas

- The code editor of [ADR-0019](adr/0019-a-code-editor-in-pluto.md), out of the plan since
  [ADR-0040](adr/0040-out-of-the-plan.md); the architecture must not rule it out.
- Office editors: agents write documents as files, and Pluto previews them.
- Joining calls; actions on a smart home.
- OpenDesign's catalog of design systems.
- A Nix build without the web client.
- The fork exists on one disk only: its branch `pluto` is pushed nowhere. A private copy would
  protect it; the owner's call.
- For the chat's memory at Stage 6 ([ADR-0020](adr/0020-the-chat.md)), read on 2026-11-17:
  [Hindsight](https://github.com/vectorize-io/hindsight) (MIT) keeps facts, experiences and
  beliefs apart, each with its time and its evidence, and recalls by fusing keyword, semantic,
  graph and time searches. Its ideas fit; its server does not (Python and PostgreSQL, a model call
  at each memory kept, consolidation in the background). LongMemEval or LoCoMo in the bench would
  show whether a memory earns its place
  ([ADR-0037](adr/0037-how-the-harness-is-judged.md)).
- Two weeks of local, private counts of which features the owner uses. Kept on the machine, never
  sent; ask before building.
- Features from other harnesses' add-ons that the owner left for later on 2026-11-17
  ([ADR-0045](adr/0045-features-from-other-harnesses-add-ons.md); those kept are in the
  [specification](specification.md#8-features-from-other-harnesses-add-ons)). Each would be built
  by Pluto itself, its source adapted only under MIT or Apache-2.0:
  - **the loop**: a map of the repository in a token budget (Aider); feedback after each edit
    (cargo check, clippy, tests and the language server, parsed); a hash-anchored edit format; a
    context compactor with no model call; terse or simple answer profiles, measured;
  - **the delegation**: a graph of tasks with waves, claims and a converge step (Kiro, Beads,
    spec-kit); a price limit before each call and a budget reserved for parallel children (Vibe,
    LiteLLM); the owner's rules in a few lines, proposed after a correction (hookify); hard tasks
    checked by fresh agents with a ledger (Claude's math-proof); a debug mode with a local log
    sink and a checked cleanup (Cursor); a filter of endpoints by their data policy (OpenRouter);
    "consult agent X" in the composer; a reminder before a quota resets (code-notify);
  - **the command line and sessions**: `pluto ask` and a shell key, and `cmd | pluto` (shell_gpt,
    aichat, llm); a worktree command (Worktrunk); cost by branch, task and pull request with a
    waste finder (CodeBurn); a session replayed as one file without secrets (claude-replay); a
    two-pass review with a simplify pass (Claude's code-review and code-simplifier); checkpoints
    linked to commits (Entire); the running app annotated from a code thread (Cursor); a
    repository wiki with evidence (openwiki); a learning mode;
  - **notifications and voice**: sounds per event and a spoken line (peon-ping); spoken approvals
    and dictation for a coding session; dictation into any app, offline (Handy);
  - **forges**: a ticket starts a thread; a pull request's template filled; issues triaged with a
    rationale and a confidence;
  - **the desktop and creative work**: a private desktop session as a tool, with takeover; the
    owner's desktop driven through Niri and AT-SPI, on the owner's leave (computer-use-linux); LFS
    and Perforce, edits refused on unlocked files (p4mcp-server); Figma and Penpot read; audio,
    MIDI and score viewers in the files panel; MusicBrainz tag proposals and voice control for
    Opium;
  - **the chat and the assistant**: the library's chunks and a query console (Msty); a model
    matchmaker from the owner's bench; chat with the open page; saved actions on a selection;
    meeting notes without a bot (against ADR-0040's calls, the owner's call); related notes by
    meaning; a compiled wiki of notes (Karpathy's LLM wiki); an inbox of documents and links
    (Paperless-AI, Karakeep);
  - **outside the rules today, ideas only**: cloud sandboxes, optional and off by default; usage
    limits read from browser cookies, against ADR-0009; the smart home through Home Assistant's
    MCP server, out of the plan by ADR-0040.

## Known debt

In [`debt.md`](debt.md), each item with why it stays and what follows it.
