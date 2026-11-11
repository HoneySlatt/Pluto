# Roadmap

Pluto's direction: what it is for, the quality bar it is held to, the decisions taken and open,
and its stages in order. The current work is in [`PLAN.md`](PLAN.md); the decisions are in
[`adr/`](adr/README.md). Update this file when a stage is reshaped, a budget changes or an open
decision is taken, each such change with its ADR.

## 1. Vision

Pluto is one place to work with every coding agent and every model. It holds Claude Code, Codex,
the other agents and local models side by side, as equals, for code and for chat, with a code
editor of its own. It aims to be the best of its market, criterion by criterion, measured
([ADR-0017](adr/0017-pluto-measured-against-the-market.md)). It is native and
frugal: it starts at once, holds little memory, draws nothing at rest, and stays smooth on a modest
laptop. It is private: nothing leaves the owner's machines but the agents' own traffic, with the
one exception the promises below name.

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

Pluto is judged against the market criterion by criterion
([ADR-0017](adr/0017-pluto-measured-against-the-market.md)):

- On each criterion, the best rival measured sets the bar, and Pluto beats it by its family's
  margin. Beating each line is the least.
- The bar is the stricter of the budget below and the figure drawn from the best rival. A rival
  never loosens a budget.
- Two comparisons: Pluto's own share, client and server, against the rival's own share, agents
  excluded; and the whole system over a day of 15 threads, agents included.
- Every rival is measured on one workload from a scripted fake model service, in a disposable,
  isolated profile; an account of the owner's only with the owner's go, account by account. The
  rivals are measured first in [M1.6](PLAN.md#m16--the-rivals-measured)
  ([ADR-0026](adr/0026-the-rivals-measured-how.md)), then again at each stage's
  end, each figure with its rival's version and date in [`measurements.md`](measurements.md).

The margins, by family:

| Family | Margin over the best rival |
|---|---|
| Resources: memory and processor at rest, wake-ups, start, binary size, disk, energy | twice better: at most half the best rival's |
| Smoothness: input to photon | twice better, timed from outside by a light sensor |
| The editor's typing | twice better on the part Pluto controls, from the key's event to the frame presented |
| The chat's own delay, the phone's budgets | the same families and margins as the desktop's |

The rivals, named only to be measured:

| Where | Rivals |
|---|---|
| Agents' applications | Claude Code's desktop application, Codex in ChatGPT's desktop application; Zed, Warp; T3 Code and today's fork, OpenCode Desktop, Emdash or Nimbalyst. On features only: Cursor, Copilot, Antigravity; Conductor on paper, closing since 2026-11-04 ([ADR-0022](adr/0022-the-triage-row-by-row.md)) |
| The editor | Zed, Neovim, Vim, Helix, Sublime Text, VS Code; Fresh for huge files; JetBrains for analysis ([ADR-0019](adr/0019-a-code-editor-in-pluto.md)) |
| The chat | ChatGPT's and Claude's desktop applications; T3 Chat, Open WebUI, LibreChat, Msty, TypingMind; LM Studio, Jan; Gemini and Le Chat on their features and voice ([ADR-0020](adr/0020-the-chat.md)) |
| The phone | Claude's and ChatGPT's remote control, T3 Code's mobile application, Happy, Paseo; Conductor and Nimbalyst on paper ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md)) |

Only first figures exist: T3 Code and Zed at rest, two runs and one, in [`measurements.md`](measurements.md#the-rivals-bench-m16a); the rest waits for the owner's go ([ADR-0027](adr/0027-the-rivals-measures-set-aside.md)), and the bar is tightened once they are measured. A survey of 2026-11-08, mostly users' reports, found every agents' application
but Zed and Warp built on Electron, the vendors' applications growing to gigabytes, and no vendor
publishing budgets.

Hardware:

- **High tier**: the owner's desktop, of the class of an RX 6900 XT, 8 cores, 32 GB, 3840 × 2160 at
  240 Hz, so 4.17 ms a frame.
- **Lowest tier**: the owner's HP Laptop 17-ca2xxx, an Athlon Silver 3050U with 2 cores and 4 GB of
  RAM, 3.25 GiB of it left to programs. It is the same machine that uwui measures first. It runs
  the client and a server with two or three agents; for the editor, it is a thin client of the
  desktop, whose server runs the language servers. Its display and storage are not yet measured.
- **Phone**: the owner's Pixel, under GrapheneOS.

Budgets, validated by the owner on 2026-11-06; six on 2026-11-07, from the measurements of
[M1.2](PLAN.md#m12--today-measured) ([ADR-0015](adr/0015-the-budgets-set-from-todays-figures.md)); hours without a
crash on 2026-11-07 ([ADR-0016](adr/0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)); the rest on
2026-11-08 ([ADR-0017](adr/0017-pluto-measured-against-the-market.md) to
[ADR-0020](adr/0020-the-chat.md)). Each may be tightened by the rivals' figures. Most are not yet met:
today's figures are beside them, and each gap is known debt.

| Budget | Target | Today | Measured by | Checked in |
|---|---|---|---|---|
| Client at rest | 0 frames, 0 wake-ups of its own a second; at most 100 MB with 15 threads open, counted as private, swap and GPU memory ([ADR-0026](adr/0026-the-rivals-measured-how.md)) | 0.05 frames and 3.2 wake-ups a second; 143 MB resident, not yet counted as ADR-0026 counts memory ([measured](measurements.md#the-client-at-rest)) | `scripts/measure.sh`: `/proc/<pid>/status`, `pidstat -w` | by hand at each milestone; CI once a headless run exists |
| Server at rest | at most 80 MB, counted as for the client, agents' processes and Chromium excluded; 0 wake-ups of its own | the fork: 394 to 892 MB, 4 to 30 wake-ups a second ([measured](measurements.md#the-fork)) | `scripts/measure.sh`: `/proc/<pid>/status`, `pidstat -w` | CI |
| Start | first useful frame warm in 100 ms, cold in 500 ms, high tier | 125 ms warm, 195 ms cold, to its painting ([measured](measurements.md#the-start)) | `examples/live.rs`, `hyperfine` | by hand at each milestone |
| Frames | 99.9 % of frames on time at the display's rate, none over 1.5 times the frame, in every scenario: a long thread of 5,000 messages, 15 threads streaming, scrolling, opening a thread, the editor; the HP at 60 Hz | the long thread: p99 8.5 ms, UI pass only, at 1440 × 900; 15 streams: p99 3.6 ms on replayed streams ([measured](measurements.md#a-long-thread-and-streams)) | `examples/bench.rs`, on recorded streams from M1.5 | by hand at each milestone |
| Input to photon | at most one frame at p99 | not measured: needs the light sensor | the light sensor bought for SlimeEngine; the compositor's presentation times for Pluto's own traces | from Stage 6, on uwui |
| Long sessions | after 24 hours and after 7 days with 15 threads open, memory and wake-ups, client and server, within 10 % of a fresh start | not measured | a long session replayed on the fake model, accelerated | at each stage's end |
| Energy | at rest and over an hour of 15 threads streaming, at most half the best rival's | not measured | the HP's battery; the desktop's processor counters | at each stage's end |
| Tokens | a turn started by Pluto spends at most 5 % more tokens than the same agent alone | not measured | the fake model counts what it receives | from Stage 2 |
| The chat's own delay | at most 10 ms added by Pluto from sending to the first word shown, same machine | not measured | the fake model | from Stage 2 |
| The HP | the client, a server and three agents without swap | not measured: its commands are ready ([the HP](measurements.md#the-hp)) | `/proc/meminfo`, `vmstat` | by hand at each stage |
| The binary's size | each binary, client and server, at most 30 MB stripped | the client, 59.5 MB stripped ([measured](measurements.md#the-budgets-m12-set)) | the stripped binary's size | by hand at each stage |
| An event's latency | server to client, p99 within 5 ms on loopback, 16 ms on the local network | not measured: the fixture runs no turn | the protocol's tests | from Stage 2 |
| A thread opened | a thread visited before shows whole in the frame after the click; a thread never opened, within 50 ms from a server on the same machine, to confirm by measure | not measured | the frame bench | from Stage 2 |
| The history's import | at most 60 s on the high tier, 5 min on the HP, resumable | not measured: 2.79 GiB to import ([measured](measurements.md#the-forks-database)) | timed on a copy of the owner's history | Stage 3 |
| An idle agent's resume | its first event within 2 s of the message sent, high tier | not measured: it needs a real agent | by the owner, by hand | from Stage 2 |
| The database's growth | at most 25 MiB a day of heavy use | about 240 MiB a day ([measured](measurements.md#the-forks-database)) | the database's size, day by day | from Stage 2 |
| Worktrees | a new worktree costs only the files git tracks; inactive worktrees never grow unseen | not measured | the worktrees' size | from Stage 2 |
| Hours without a crash | at least 99.9 % of hours of use without a crash, client and server; no work lost after one, a draft in the composer included | not measured: neither hours nor crashes are counted | the hours of use and the crashes, counted on the machine | from Stage 2 for the server, from Stage 6 for the client |
| The editor's typing | a keystroke shows in the next frame | not measured | the light sensor, on `sqlite3.c` and `gcc.c` | Stage 7 |
| The phone | cold start, frames, the battery over an hour of a thread streaming, data per hour, memory: the desktop's families and margins | not measured | Macrobenchmark and Perfetto on the Pixel | from Stage 5 |

Guarantees, each with its test ([ADR-0017](adr/0017-pluto-measured-against-the-market.md)):
- an update or a restart of the server never stops a running agent;
- a dropped connection recovers within one second on the local network, losing nothing;
- a worktree with uncommitted work is never deleted;
- an update of Pluto costs no thread and no draft;
- every first-rank agent has steering, interrupt, resume, checkpoints, approvals, the plan and its
  usage, native or supplied by Pluto, checked by a grid of tests on recorded sessions.

Promises:
- instant: no common action on the desktop shows a loading state;
- nothing is ever lost;
- no account and no cloud for Pluto itself, on the local network and offline; only the agents' own
  traffic leaves, and an answer's safe images and link icons
  ([ADR-0022](adr/0022-the-triage-row-by-row.md));
- Linux first-rank: Wayland, sharp fractional scales, the HP smooth, Linux checked first at each
  release.

Also held, without a figure:
- WCAG 2.2 AA: a screen reader, the whole application by the keyboard, the editor included,
  contrasts checked, reduced motion and transparency;
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
| Judged against the market | The best rival measured on each criterion sets the bar, beaten by a margin per family; the stricter of that and the budgets ([ADR-0017](adr/0017-pluto-measured-against-the-market.md)) |
| The market's features, the triage, the phone | Review in Pluto, triggers, one configuration for every agent, side questions, dictation, cloud runs, computer use in an isolated session; families marked; the phone's notifications through UnifiedPush, its own models ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md)) |
| The features kept | Today's features triaged row by row; the specification accepted ([ADR-0022](adr/0022-the-triage-row-by-row.md), [ADR-0023](adr/0023-the-specification-accepted.md), [`specification.md`](specification.md)) |
| The editor | In Pluto, optional, on uwui's component, its core in the server, Stage 7 ([ADR-0019](adr/0019-a-code-editor-in-pluto.md)) |
| The chat | It replaces the vendors' applications; each vendor by its permitted path; one memory, local voice, one history ([ADR-0020](adr/0020-the-chat.md)) |
| uwui's decisions of 2026-11-07 | Agents reach uwui's applications only where the sandbox allows it; what only a person may do is never offered to agents; uwui's rules for applications in the bar; macOS decided when Pluto goes public ([ADR-0016](adr/0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)) |

### Open

Each is decided by an ADR when it becomes necessary, not earlier.

| Topic | Candidates | Decide at |
|---|---|---|
| The protocol's wire format and transports | JSON over WebSocket; a binary encoding (CBOR, postcard); QUIC through iroh away from home | Stage 2, the protocol's milestone |
| The storage's model | an event log, or plain tables | Stage 2, the storage's milestone |
| A keyring without a graphical session | systemd's credentials; a file sealed by the session's keyring; another | Stage 2, the secrets' milestone |
| DeepSeek Harness in Pluto's own server | native, ACP, or a small Node module if its only interface is TypeScript; the fork already drives its local Web protocol ([ADR-0021](adr/0021-deepseek-harness-in-todays-pluto.md)) | when its interface is stable, at the latest Stage 3 |
| When the client moves to uwui | when uwui takes its pilots, from uwui's plan | uwui's first stage |
| A thread moved between machines | how, and whether | after Stage 5 |
| Going public | each provider's terms, kept in the table of the vendors' paths ([ADR-0020](adr/0020-the-chat.md)); the name; Flatpak, AppImage, Windows packages; macOS, the client through uwui and the server's sandbox, keyring and terminals ported; the sandbox on Windows and Android; editing with other people in the editor | Stage 8 |
| Gemini through its official CLI | allowed or not by Google, which has not answered; an API key until then | when Google answers |
| Agents' sessions kept ready for the chat | how many, and for how long, against "nothing runs at rest" | Stage 2, measured |
| The bench's tasks | a subset of Terminal-Bench through Harbor; SWE-bench Verified Mini; tasks of Pluto's own | Stage 4, its first milestone |
| The library of the server's MCP tools | the official Rust SDK (`rmcp`); uwui's MCP library, only if it pulls no crate of uwui's interface; Pluto's own | Stage 3, the MCP tools' milestone |
| Pluto as the owner's assistant on the desktop, calling the actions that uwui's applications declare, where Linux has no such mechanism | built or not, and how | after Stage 4, once uwui's local MCP server exists |
| A command-line client | when, and how much of the protocol it covers | after Stage 4 |
| Pluto's own extensions beyond the editor's; OpenDesign's catalog | built or not, and when | the owner's go; extensions after uwui's 1.0 |

## 4. Stages

### Stage 1 — The ground measured and triaged

**Goal.** Start the new direction from facts: Pluto under the method, today measured, and the
features triaged with the owner.

- `scripts/check.sh` and CI; `docs/ARCHITECTURE.md` of the code as it is.
- The client, the server and the HP measured; what fills the fork's database examined on a copy.
- Every feature inventoried and triaged; the egui client and the Android app cut accordingly.
- Agents' sessions recorded, for the drivers' replay tests.
- The rivals measured on a fake model service, so that the server starts from their figures
  ([ADR-0017](adr/0017-pluto-measured-against-the-market.md)).

**Exit.** The triage is recorded as the specification. Today's figures are in
`docs/measurements.md`. The checks run in CI. The cuts are in daily use. The rivals' figures wait
for the owner's go ([ADR-0027](adr/0027-the-rivals-measures-set-aside.md)).

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
- The sandbox and the masking of secrets; reaching uwui's applications is a right of the sandbox,
  closed when it is locked ([ADR-0016](adr/0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)).
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

- The rivals measured again, at their versions of the day.
- The desktop client on Pluto's protocol, with Pluto's theme over uwui's own look, the dashboard
  and keyboard-first use. Its actions for agents keep for the person what only a person may do.
- WCAG 2.2 AA.
- Then the phone, then the web.

Its start follows uwui's plan, and may overlap Stages 3 to 5.

**Exit.** The egui client is removed. Every budget is met on the high tier and on the HP.

### Stage 7 — The editor

**Goal.** A code editor in Pluto better than Zed and Neovim on modal editing, language
intelligence, remote editing and extensions ([ADR-0019](adr/0019-a-code-editor-in-pluto.md)).

- Its core in Pluto's server, on every machine, Windows, containers and virtual machines included.
- Two modal engines, Vim's and Helix's, held to the real programs in CI; the owner's Neovim
  imported.
- A live merge of the owner's and the agents' edits; agents' carets, review in the file, edits
  predicted.
- Language servers, debuggers, huge files, search at ripgrep's speed, a hexadecimal editor; a
  terminal mode.
- Extensions in steps, isolated.

It is Pluto's largest stage, split into milestones when it starts, on the owner's go.

**Exit.** The owner edits in Pluto every day, and the editor's budgets are met against its rivals.

### Stage 8 — Going public

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
| 0.7 | 7 | The editor |
| 1.0 | 8 | Public |

Releases before 1.0 are private: a version, a CHANGELOG and a tag, after the owner's go.
