# Plan

What is being worked on now: the milestones of the current stage, their status, how each one is
checked. Direction beyond this stage is in [`ROADMAP.md`](ROADMAP.md); decisions are in
[`adr/`](adr/README.md); the stages done are in [`history.md`](history.md); the known debt is in
[`debt.md`](debt.md). A *stage* is a step of the roadmap; a *milestone* is a small, verifiable step
of a stage.

Every milestone ends with the definition of done in [`AGENTS.md`](../AGENTS.md#definition-of-done).

## Current

- **On 2026-11-15 Pluto became a harness.** A design session with the owner decided it; its
  decisions, and the lead's implementation choices marked as such, are
  [ADR-0030](adr/0030-pluto-becomes-a-harness.md) to [ADR-0040](adr/0040-out-of-the-plan.md), and
  the new stages are in the
  [ROADMAP](ROADMAP.md#4-stages).
- **Stage 1's milestones are done or moved** ([`history.md`](history.md)). Stage 1 closes with its
  stage audit, which comes next, in a session of its own, following the method's `audits.md`; the
  fate of the unpushed branch `m16b2b` is one of its questions. Then
  [M2.0](#m20--the-new-foundations-architecture).
- The egui client, the Android app and the fork run unchanged until the end of Stage 5
  ([ADR-0038](adr/0038-a-new-foundation-beside-the-fork.md)). DeepSeek Harness is a provider of
  the fork ([ADR-0021](adr/0021-deepseek-harness-in-todays-pluto.md)).
- What Pluto asks of uwui is in [`uwui-needs.md`](uwui-needs.md). The owner expects uwui to be ready
  before Stage 3.

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
  contribute (the owner's request of 2026-11-16).
- **Decided first.** A design session, without code: how Codex, goose, Zed and OpenCode lay out
  their crates and extension points; the options; a challenger; the decisions put to the owner one
  at a time, then ADRs.
- **Delivers.**
  - In [`ARCHITECTURE.md`](ARCHITECTURE.md), the target: the crates of Stages 2 to 5 by name, what
    each holds, the direction of their dependencies, and each one's public surface.
  - The seams as contracts: the session log's schema, the provider client, the engine, the tool;
    a recipe to add a provider, a tool or an engine, each in one place.
  - A check in `scripts/check.sh` that refuses a dependency against the direction.
  - The frozen egui client and its crates in a workspace of their own, so that the new foundation
    builds and tests without them.
  - `CONTRIBUTING.md`: the environment, the commands, where to start.
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
  repaired: options put to the owner, then an ADR.
- **Delivers.**
  - `crates/pluto-log`: the entries (messages, tool calls and results, usage, context edits,
    fetched pages, provenance), versioned from the first line; an append-only writer whose
    appends are atomic; a torn last entry repaired on opening; a reader; the context view.
  - A converter from Stage 1's recordings (Claude Code's `stream-json`, Grok's ACP) into logs.
- **Done when.** Each recording, real or synthetic, converts into a log whose view gives the
  scenario's reference exactly. The tests cover an empty log, a huge one, malformed and torn
  entries, a second writer refused, and a newer version refused with a plain message.
- **How to test.** `cargo test -p pluto-log`, in `scripts/check.sh`.
- **Measured.** An append's time; a log's size per turn on the recordings, against the storage
  budget of 25 MiB a day ([ROADMAP](ROADMAP.md#quality-bar)).

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
    model's endpoint.
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
- Two weeks of local, private counts of which features the owner uses. Kept on the machine, never
  sent; ask before building.

## Known debt

In [`debt.md`](debt.md), each item with why it stays and what follows it.
