# Roadmap

Pluto's direction: what it is for, the quality bar it is held to, the decisions taken and open,
and its stages in order. The current work is in [`PLAN.md`](PLAN.md); the decisions are in
[`adr/`](adr/README.md). Update this file when a stage is reshaped, a budget changes or an open
decision is taken, each such change with its ADR.

## 1. Vision

Pluto is one place to work with every coding agent and every model. It holds Claude Code, Codex,
the other agents and local models side by side, as equals, for code and for chat. It is native and
frugal: it starts at once, holds little memory, draws nothing at rest, and stays smooth on a modest
laptop. It is private: nothing leaves the owner's machines but the agents' own traffic.

It is made for its owner first, who runs 5 to 15 threads on a busy day, and designed so it can go
public later. It is one of the programs of reference of uwui, the owner's UI framework, which its
client moves to. Its decisions are in [ADR-0003](adr/0003-what-pluto-is-to-become.md).

## 2. Principles

- Fewer features, each done well. A feature earns its place in the triage
  ([ADR-0010](adr/0010-todays-features-triaged.md)).
- Measured, not claimed: every budget below has its benchmark.
- Nothing runs, polls or draws while there is nothing to do, in the client, in the server, or in an
  idle agent.
- The owner's data is never lost: versioned formats, atomic writes, backups before migrations.
- No real provider turn in automated checks.
- Every step in daily use: the owner works with Pluto at the end of each milestone.

## Quality bar

References, as surveyed on 2026-11-06:

- **Zed**: native speed and the Agent Client Protocol. Its agent panel's memory grows with long
  threads.
- **The Codex app**: orchestration, an in-app browser, automations. It is Electron, at 1.3 to 2.2 GB
  at rest.
- **Claude's desktop app**: review with batched line comments, environments and remote control.
- **Conductor**: polish, worktrees created at once. It runs on macOS only.
- **T3 Code**: many providers through one server and thin clients. Pluto's starting point.

Pluto aims to beat each of them on what it does best, at a fraction of their memory, on Linux first.

Hardware:

- **High tier**: the owner's desktop, of the class of an RX 6900 XT, 8 cores, 32 GB, 3840 × 2160 at
  240 Hz, so 4.17 ms a frame.
- **Lowest tier**: the owner's HP Laptop 17-ca2xxx, an Athlon Silver 3050U with 2 cores and 4 GB of
  RAM, 3.25 GiB of it left to programs. It is the same machine that uwui measures first. It runs
  the client and a server with two or three agents. Its display and storage are not yet measured.
- **Phone**: the owner's Pixel, under GrapheneOS.

Budgets, validated by the owner on 2026-11-06; the last six on 2026-11-07, from the measurements of
[M1.2](PLAN.md#m12--today-measured) ([ADR-0015](adr/0015-the-budgets-set-from-todays-figures.md)).
Most are not yet met: today's figures are beside them, and each gap is known debt.

| Budget | Target | Today | Measured by | Checked in |
|---|---|---|---|---|
| Client at rest | 0 frames, 0 wake-ups of its own a second; at most 100 MB resident with 15 threads open | 0.05 frames and 3.2 wake-ups a second; 143 MB ([measured](measurements.md#the-client-at-rest)) | `scripts/measure.sh`: `/proc/<pid>/status`, `pidstat -w` | by hand at each milestone; CI once a headless run exists |
| Server at rest | at most 80 MB resident, agents' processes and Chromium excluded; 0 wake-ups of its own | the fork: 394 to 892 MB, 4 to 30 wake-ups a second ([measured](measurements.md#the-fork)) | `scripts/measure.sh`: `/proc/<pid>/status`, `pidstat -w` | CI |
| Start | first useful frame warm in 100 ms, cold in 500 ms, high tier | 125 ms warm, 195 ms cold, to its painting ([measured](measurements.md#the-start)) | `examples/live.rs`, `hyperfine` | by hand at each milestone |
| A long thread | 5,000 messages scroll at the display's rate: 240 Hz on the high tier (p99 4.17 ms), 60 Hz on the HP | p99 8.5 ms, UI pass only, at 1440 × 900 ([measured](measurements.md#a-long-thread-and-streams)) | `examples/bench.rs` | by hand at each milestone |
| Streaming | 15 threads streaming at once, no frame over budget at p99 | p99 3.6 ms, UI pass only, at 1440 × 900, on replayed streams ([measured](measurements.md#a-long-thread-and-streams)) | the bench, on recorded streams from M1.5 | by hand at each milestone |
| The HP | the client, a server and three agents without swap | not measured: its commands are ready ([the HP](measurements.md#the-hp)) | `/proc/meminfo`, `vmstat` | by hand at each stage |
| Input to present | within one frame at p99, as uwui's | not measured: the client does not ask for the compositor's presentation times | the compositor's presentation feedback | from Stage 6, on uwui |
| The binary's size | each binary, client and server, at most 30 MB stripped | the client, 59.5 MB stripped ([measured](measurements.md#the-budgets-m12-set)) | the stripped binary's size | by hand at each stage |
| An event's latency | server to client, p99 within 5 ms on loopback, 16 ms on the local network | not measured: the fixture runs no turn | the protocol's tests | from Stage 2 |
| The history's import | at most 60 s on the high tier, 5 min on the HP, resumable | not measured: 2.79 GiB to import ([measured](measurements.md#the-forks-database)) | timed on a copy of the owner's history | Stage 3 |
| An idle agent's resume | its first event within 2 s of the message sent, high tier | not measured: it needs a real agent | by the owner, by hand | from Stage 2 |
| The database's growth | at most 25 MiB a day of heavy use | about 240 MiB a day ([measured](measurements.md#the-forks-database)) | the database's size, day by day | from Stage 2 |

Also held, without a figure:
- WCAG 2.2 AA: a screen reader, the whole application by the keyboard, contrasts checked, reduced
  motion and transparency;
- no telemetry;
- bad input from agents or servers never crashes the client or the server;
- secrets never in a log, an export or a capture.

## 3. Decisions

### Fixed

| Topic | Decision |
|---|---|
| The client | Native Rust; egui, then uwui as one of its pilots ([ADR-0001](adr/0001-a-native-client-on-egui-until-uwui.md)) |
| The phone | Kotlin and Compose over the Rust core, then uwui ([ADR-0002](adr/0002-the-android-app-until-uwui.md)) |
| What Pluto is | [ADR-0003](adr/0003-what-pluto-is-to-become.md) |
| The server | Rust, no Node, in front of the frozen fork, Claude first ([ADR-0004](adr/0004-plutos-own-server-in-rust.md)) |
| The protocol | Pluto's own, typed, versioned ([ADR-0005](adr/0005-plutos-own-typed-protocol.md)) |
| Drivers | Native where it pays, ACP otherwise, no Cursor ([ADR-0006](adr/0006-agents-drivers.md)) |
| Machines | Several servers in one view, a relay on iroh ([ADR-0007](adr/0007-several-servers-and-a-relay.md)) |
| Storage | SQLite, versioned; the history imported as final states ([ADR-0008](adr/0008-storage-and-the-history-moved.md)) |
| Safety | Keyring, per-device keys, sandbox, masking, no telemetry ([ADR-0009](adr/0009-secrets-privacy-and-the-sandbox.md)) |
| Features | Today's, triaged; egui frozen ([ADR-0010](adr/0010-todays-features-triaged.md)) |
| License | MIT OR Apache-2.0 for new code ([ADR-0003](adr/0003-what-pluto-is-to-become.md)) |
| Pluto's own agent | A minimal harness, measured on Pluto's bench, after Stage 3 ([ADR-0011](adr/0011-plutos-own-harness-minimal-and-measured.md)) |

### Open

Each is decided by an ADR when it becomes necessary, not earlier.

| Topic | Candidates | Decide at |
|---|---|---|
| The features kept | the triage of today's features | [M1.3](PLAN.md#m13--the-triage) |
| The protocol's wire format and transports | JSON over WebSocket; a binary encoding (CBOR, postcard); QUIC through iroh away from home | Stage 2, the protocol's milestone |
| The storage's model | an event log, or plain tables | Stage 2, the storage's milestone |
| A keyring without a graphical session | systemd's credentials; a file sealed by the session's keyring; another | Stage 2, the secrets' milestone |
| DeepSeek Harness | native, ACP, or a small Node module if its only interface is TypeScript | when its interface is stable, at the latest Stage 3 |
| When the client moves to uwui | when uwui takes its pilots, from uwui's plan | uwui's first stage |
| The phone's notifications | a kept connection, as today; through the relay with a push service without Google's | Stage 5, the relay's milestone |
| A thread moved between machines | how, and whether | after Stage 5 |
| Going public | each provider's terms (Anthropic forbids third-party products to offer claude.ai sign-in without approval); the name; Flatpak, AppImage, Windows; the sandbox on Windows and Android | Stage 7 |
| The bench's tasks | a subset of Terminal-Bench through Harbor; SWE-bench Verified Mini; tasks of Pluto's own | Stage 4, its first milestone |
| A command-line client | when, and how much of the protocol it covers | after Stage 4 |
| Voice dictation; extensions; OpenDesign's catalog | built or not, and when | the owner's go; extensions after uwui's 1.0 |

## 4. Stages

### Stage 1 — The ground measured and triaged

**Goal.** Start the new direction from facts: Pluto under the method, today measured, and the
features triaged with the owner.

- `scripts/check.sh` and CI; `docs/ARCHITECTURE.md` of the code as it is.
- The client, the server and the HP measured; what fills the fork's database examined on a copy.
- Every feature inventoried and triaged; the egui client and the Android app cut accordingly.
- Agents' sessions recorded, for the drivers' replay tests.

**Exit.** The triage is recorded as the specification. Today's figures are in
`docs/measurements.md`. The checks run in CI. The cuts are in daily use.

### Stage 2 — Pluto's server, its first agents

**Goal.** The owner works through Pluto's own server every day, with Claude on it.

- The protocol's crate, versioned.
- The server: storage, threads, connections and pairing on the local network, secrets in the
  keyring.
- Claude's driver, and one ACP agent.
- Terminals, git and worktrees, files.
- The gateway that forwards everything else to the frozen fork.
- The egui client and the Android app on Pluto's protocol.

**Exit.** Every client speaks only Pluto's protocol. Claude's threads run on the Rust server, the
others through the fork behind it. The server's budgets are measured.

### Stage 3 — Every agent on Pluto's server, the fork stopped

**Goal.** Node no longer runs.

- Every first-rank driver, Ollama's model management among them.
- The chat space, its memory and projects, and design projects.
- Pluto's MCP tools, scheduled tasks, usage and quotas.
- The browser through CDP.
- The sandbox and the masking of secrets.
- The history imported and verified.

**Exit.** The fork is stopped, and the history is imported and checked. The budgets are met on the
high tier and measured on the HP.

### Stage 4 — Pluto's own agent

**Goal.** Local models and API keys code through Pluto's own harness, better than through OpenCode
([ADR-0011](adr/0011-plutos-own-harness-minimal-and-measured.md)).

- Pluto's bench: a small fixed set of tasks, run on the owner's local models.
- The harness: a small core, context management, a plan tool, loop detection, tolerant edits, a
  profile per model; Pluto's server's tools offered to it.
- Adapters for Ollama's native API, Chat Completions and Anthropic's Messages.

**Exit.** The harness beats the OpenCode companion on the bench, on the owner's local models, and
replaces it.

### Stage 5 — Several machines, and agents together

**Goal.** Pluto's differences, on the server's side.

- Several servers in one view, devices paired, the relay on iroh, the phone's notifications.
- The data behind the dashboard: resources per agent and per machine.
- Warnings before a quota runs out, and the suggested switch.
- Delegation between providers, cross-agent review, and comparison of several models, in that
  order.

**Exit.** The owner works from the laptop and the phone away from home, and each difference is in
daily use.

### Stage 6 — The client on uwui

**Goal.** Pluto's client, written anew on uwui from the triaged specification, as one of uwui's
pilots.

- The desktop client on Pluto's protocol, with Pluto's theme, the dashboard and keyboard-first use.
- WCAG 2.2 AA.
- Then the phone, then the web.

Its start follows uwui's plan, and may overlap Stages 3 to 5.

**Exit.** The egui client is removed. Every budget is met on the high tier and on the HP.

### Stage 7 — Going public

**Goal.** Pluto opens to others, on the owner's go.

- The open decisions of going public taken.
- Packages beyond Nix; Windows.
- A release audit.

**Exit.** A public release, after the owner's go.

## 5. Release map

| Version | Stage | Content |
|---|---|---|
| 0.2 | 2 | Pluto's server in daily use, Claude on it |
| 0.3 | 3 | The fork stopped |
| 0.4 | 4 | Pluto's own agent |
| 0.5 | 5 | Several machines, agents together |
| 0.6 | 6 | The client on uwui |
| 1.0 | 7 | Public |

Releases before 1.0 are private: a version, a CHANGELOG and a tag, after the owner's go.
