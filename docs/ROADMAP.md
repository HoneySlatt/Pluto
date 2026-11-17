# Roadmap

Pluto's direction: what it is for, the quality bar it is held to, the decisions taken and open,
and its stages in order. The current work is in [`PLAN.md`](PLAN.md); the decisions are in
[`adr/`](adr/README.md); the stages done are in [`history.md`](history.md). Update this file when a
stage is reshaped, a budget changes or an open decision is taken, each such change with its ADR.

## 1. Vision

Pluto is a harness of its own: one place to work with every model and every coding agent, for code,
chat, design and orchestration, and later a personal assistant
([ADR-0030](adr/0030-pluto-becomes-a-harness.md)). Its loop, its tools, its prompts and its context
are Pluto's, so the owner controls the brain, agents behave alike, and the vendors' churn stops
mattering. Where a vendor's own engine is better or is the only path its terms allow, Pluto drives
it under one thread model ([ADR-0031](adr/0031-three-kinds-of-engine.md)).

It aims to be the best of its market, criterion by criterion, measured
([ADR-0017](adr/0017-pluto-measured-against-the-market.md)). Its harness matches the vendors' on
frontier models for fewer tokens, and leads on local and cheap ones
([ADR-0037](adr/0037-how-the-harness-is-judged.md)). It is native and frugal: it starts at once,
holds little memory, draws nothing at rest, and stays smooth on a modest laptop. It is private:
nothing leaves the owner's machines but the models' own traffic, with the one exception the
promises below name.

It is made for its owner first, who runs 5 to 15 threads on a busy day, and designed so it can go
public later. Its surfaces are a desktop client on uwui, the owner's UI framework, a phone that
does everything, a command line, and later a light TUI for machines without a screen ([ADR-0042](adr/0042-a-cli-first-the-tui-with-the-assistant.md)). It is one of uwui's programs of reference.
It targets Linux, Windows, macOS, Android and iOS: Linux and Android first, the three others at
going public ([ADR-0044](adr/0044-five-platforms-at-going-public.md)).

## 2. Principles

- Fewer features, each done well. A feature earns its place in the triage
  ([ADR-0010](adr/0010-todays-features-triaged.md)).
- Measured, not claimed: every budget below has its benchmark.
- Nothing runs, polls or draws while there is nothing to do: in a client, in the server, in an idle
  agent. The assistant runs only between its triggers' firing and its answer.
- The owner's data is never lost: versioned formats, atomic writes, backups before migrations.
- No real provider turn in automated checks.
- Every step in daily use: the owner works with Pluto at the end of each milestone. From Stage 3,
  each milestone names the slice of the owner's work it moves onto the new foundation
  ([ADR-0038](adr/0038-a-new-foundation-beside-the-fork.md)).

How the harness stays able to change: twelve rules drawn from a survey of 2026-11-15 of how the
best harnesses evolved (Anthropic, OpenAI's Codex, Cursor, Amp, OpenCode, Pi, Hermes, MCP), which
the owner asked to hold as principles. Where an ADR decides one, it is linked:

1. **Each component is an assumption about what the model cannot do.** It is tested again at each
   model's release, and deleted when it no longer pays.
2. **What depends on a model lives in its profile** (prompt, tools, effort, edit format), not in
   code paths, with a switch to restore anything removed.
3. **A session is an append-only, versioned log**, outside the loop and the interface. The context
   is a view of it ([ADR-0033](adr/0033-one-pluto-agent-process-per-session.md)).
4. **Every protocol is versioned** and its capabilities negotiated
   ([ADR-0005](adr/0005-plutos-own-typed-protocol.md)); its schemas are generated from one source,
   its unstable parts behind an opt-in.
5. **A small kernel**; everything else in pieces that can be turned off or replaced.
6. **Capabilities are plain files**, loaded in layers: skills, hooks, named agents, the method
   itself ([ADR-0034](adr/0034-the-methods-mechanisms-native.md)). Each file has an owner, and is
   checked for staleness.
7. **A change to a session's context appends a small delta by default**, so the prompt cache
   holds. Rewriting the system prompt or the past shows its price first and waits for the owner's
   go ([ADR-0036](adr/0036-the-inspector-appends-by-default.md)).
8. **The bench gates every change**: to the loop, a profile, a vendor's new version
   ([ADR-0037](adr/0037-how-the-harness-is-judged.md)).
9. **Verification is a role that can be switched**, its cost measured: a checker only where it pays.
10. **Providers sit behind a thin layer driven by data**: a model registry and a table of quirks
    ([ADR-0032](adr/0032-plutos-loop-and-its-provider-clients.md)).
11. **The brain is kept apart from the hands and from credentials**: tools in a sandbox, secrets in
    a vault it cannot read ([ADR-0035](adr/0035-the-sandbox-and-the-vault.md)).
12. **Agents write skills only as files the owner reviews**, reversible and measured, never through
    a learning loop of their own.

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
  isolated profile; an account of the owner's only with the owner's go, account by account
  ([ADR-0026](adr/0026-the-rivals-measured-how.md)). The rivals' campaigns resume at Stage 5, once a
  Pluto client runs on Pluto's own server ([ADR-0039](adr/0039-the-order-of-the-stages.md)), then
  again at each stage's end, each figure with its rival's version and date in
  [`measurements.md`](measurements.md).

The harness has its own rule ([ADR-0037](adr/0037-how-the-harness-is-judged.md)):
- on frontier models, the vendor's own harness matched within the error margin, for fewer tokens;
- on local and cheap models, a measured lead;
- measured on Pluto's bench, saved briefs replayed with their hidden checks, and on a subset of
  Terminal-Bench, every figure with its confidence interval, within a fixed monthly budget for runs
  that the owner sets.

The margins, by family:

| Family | Margin over the best rival |
|---|---|
| Resources: memory and processor at rest, wake-ups, start, binary size, disk, energy | twice better: at most half the best rival's |
| Smoothness: input to photon | twice better, timed from outside by a light sensor |
| The chat's own delay, the phone's budgets | the same families and margins as the desktop's |

The rivals, named only to be measured:

| Where | Rivals |
|---|---|
| The harness | Claude Code, Codex, OpenCode, Pi; Kimi Code CLI, Mistral Vibe, DeepSeek Harness; the public boards: Terminal-Bench, Artificial Analysis's coding agents ([ADR-0037](adr/0037-how-the-harness-is-judged.md)) |
| Agents' applications | Claude Code's desktop application, Codex in ChatGPT's desktop application; Zed, Warp; T3 Code and today's fork, OpenCode Desktop, Emdash or Nimbalyst. On features only: Cursor, Copilot, Antigravity; Conductor on paper, closing since 2026-11-04 ([ADR-0022](adr/0022-the-triage-row-by-row.md)) |
| The chat | ChatGPT's and Claude's desktop applications; T3 Chat, Open WebUI, LibreChat, Msty, TypingMind; LM Studio, Jan; Gemini and Le Chat on their features and voice ([ADR-0020](adr/0020-the-chat.md)) |
| The phone | Claude's and ChatGPT's remote control, T3 Code's mobile application, Happy, Paseo; Conductor and Nimbalyst on paper ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md)) |
| The assistant | OpenClaw, Hermes, Claude Duet, ChatGPT's Dot, Grok Bot; ZeroClaw and IronClaw, in Rust ([ADR-0030](adr/0030-pluto-becomes-a-harness.md)) |

Only provisional figures exist: T3 Code and Zed at rest and streaming, from runs on a loaded
machine, in [`measurements.md`](measurements.md#the-rivals-bench-m16b1-provisional). A survey of
2026-11-08, mostly users' reports, found every agents' application but Zed and Warp built on
Electron, the vendors' applications growing to gigabytes, and no vendor publishing budgets.

Hardware:

- **High tier**: the owner's desktop, of the class of an RX 6900 XT, 8 cores, 32 GB, 3840 × 2160 at
  240 Hz, so 4.17 ms a frame.
- **Lowest tier**: the owner's HP Laptop 17-ca2xxx, an Athlon Silver 3050U with 2 cores and 4 GB of
  RAM, 3.25 GiB of it left to programs. It is the same machine that uwui measures first. It runs
  the client and a server with two or three agents. Its display and storage are not yet measured.
- **Phone**: the owner's Pixel, under GrapheneOS.

Budgets, validated by the owner on 2026-11-06; six on 2026-11-07, from the measurements of
[M1.2](history.md#m12--today-measured) ([ADR-0015](adr/0015-the-budgets-set-from-todays-figures.md));
hours without a crash on 2026-11-07
([ADR-0016](adr/0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)); the rest on 2026-11-08
([ADR-0017](adr/0017-pluto-measured-against-the-market.md) to [ADR-0020](adr/0020-the-chat.md)).
Each may be tightened by the rivals' figures. Most are not yet met: today's figures are beside
them, and each gap is known debt. The budgets of `pluto-agent` and its command line are set at Stage 3
from their first figures, the TUI's at Stage 7.

| Budget | Target | Today | Measured by | Checked in |
|---|---|---|---|---|
| Client at rest | 0 frames, 0 wake-ups of its own a second; at most 100 MB with 15 threads open, counted as private, swap and GPU memory ([ADR-0026](adr/0026-the-rivals-measured-how.md)) | 0.05 frames and 3.2 wake-ups a second; 143 MB resident ([measured](measurements.md#the-client-at-rest)); counted as ADR-0026 counts memory, about 405 MiB, 340 of it GPU memory, after 15 threads ([provisional](measurements.md#the-rivals-bench-m16b1-provisional)) | `scripts/measure.sh`: `/proc/<pid>/status`, `pidstat -w` | from Stage 5, on uwui, by hand at each milestone; CI once a headless run exists; the TUI's from Stage 7 |
| Server at rest | at most 80 MB, counted as for the client, agents' processes and Chromium excluded; 0 wake-ups of its own | the fork: 394 to 892 MB, 4 to 30 wake-ups a second ([measured](measurements.md#the-fork)) | `scripts/measure.sh`: `/proc/<pid>/status`, `pidstat -w` | CI, from Stage 5 |
| Start | first useful frame warm in 100 ms, cold in 500 ms, high tier | 125 ms warm, 195 ms cold, to its painting ([measured](measurements.md#the-start)) | `examples/live.rs`, `hyperfine` | from Stage 5, on uwui, by hand at each milestone; the TUI's from Stage 7 |
| Frames | 99.9 % of frames on time at the display's rate, none over 1.5 times the frame, in every scenario: a long thread of 5,000 messages, 15 threads streaming, scrolling, opening a thread; the HP at 60 Hz | the long thread: p99 8.5 ms, UI pass only, at 1440 × 900; 15 streams: p99 3.6 ms on replayed streams ([measured](measurements.md#a-long-thread-and-streams)) | `examples/bench.rs`, on recorded streams from M1.5 | from Stage 5, on uwui, by hand at each milestone; the TUI's from Stage 7 |
| Input to photon | at most one frame at p99 | not measured: needs the light sensor | the light sensor bought for SlimeEngine; the compositor's presentation times for Pluto's own traces | from Stage 5, on uwui |
| Long sessions | after 24 hours and after 7 days with 15 threads open, memory and wake-ups, client and server, within 10 % of a fresh start | not measured | a long session replayed on the fake model, accelerated | at each stage's end |
| Energy | at rest and over an hour of 15 threads streaming, at most half the best rival's | not measured | the HP's battery; the desktop's processor counters | at each stage's end |
| Tokens | Pluto's loop: fewer tokens than the vendor's harness on the same task, at an equal score; a vendor's engine started by Pluto: at most 5 % more tokens than the same agent alone ([ADR-0037](adr/0037-how-the-harness-is-judged.md)) | not measured | the bench; the fake model counts what it receives | the vendors' reference from Stage 2; Pluto's loop from Stage 3; vendors' engines started by Pluto from Stage 4 |
| The harness's score | on frontier models, the vendor's harness within the error margin; on local and cheap models, a measured lead | not measured | Pluto's bench and a subset of Terminal-Bench | from Stage 3 |
| The chat's own delay | at most 10 ms added by Pluto from sending to the first word shown, same machine | not measured | the fake model | from Stage 6 |
| The HP | the client, a server and three agents without swap | not measured: its commands are ready ([the HP](measurements.md#the-hp)) | `/proc/meminfo`, `vmstat` | by hand at each stage |
| The binary's size | each binary, client and server, at most 30 MB stripped | the client, 59.5 MB stripped ([measured](measurements.md#the-budgets-m12-set)) | the stripped binary's size | by hand at each stage |
| An event's latency | server to client, p99 within 5 ms on loopback, 16 ms on the local network | not measured: the fixture runs no turn | the protocol's tests | from Stage 5 |
| A thread opened | a thread visited before shows whole in the frame after the click; a thread never opened, within 50 ms from a server on the same machine, to confirm by measure | not measured | the frame bench | from Stage 5 |
| The history's import | at most 60 s on the high tier, 5 min on the HP, resumable | not measured: 2.79 GiB to import ([measured](measurements.md#the-forks-database)) | timed on a copy of the owner's history | Stage 5 |
| An idle agent's resume | its first event within 2 s of the message sent, high tier | not measured: it needs a real agent | by the owner, by hand | from Stage 3 |
| The storage's growth | sessions' logs and the server's database together, at most 25 MiB a day of heavy use | the fork: about 240 MiB a day ([measured](measurements.md#the-forks-database)) | their size, day by day | from Stage 3 |
| Worktrees | a new worktree costs only the files git tracks; inactive worktrees never grow unseen | not measured | the worktrees' size | from Stage 3 |
| Hours without a crash | at least 99.9 % of hours of use without a crash; no work lost after one, a draft in the composer included | not measured: neither hours nor crashes are counted | the hours of use and the crashes, counted on the machine | from Stage 3 for `pluto-agent`, Stage 5 for the server and the client |
| The phone | cold start, frames, the battery over an hour of a thread streaming, data per hour, memory: the desktop's families and margins | not measured | Macrobenchmark and Perfetto on the Pixel | from Stage 5 |

Guarantees, each with its test ([ADR-0017](adr/0017-pluto-measured-against-the-market.md)):
- an update or a restart of the server never stops a running agent;
- a dropped connection recovers within one second on the local network, losing nothing;
- a worktree with uncommitted work is never deleted;
- an update of Pluto costs no thread and no draft;
- every first-rank engine has steering, interrupt, resume, checkpoints, approvals, the plan and its
  usage, native or supplied by Pluto, checked by a grid of tests on recorded sessions;
- the assistant never deletes without a way back, and never spends money or sends a message without
  an approval that shows the action, the target and every recipient
  ([ADR-0030](adr/0030-pluto-becomes-a-harness.md)).

Promises:
- instant: no common action on the desktop shows a loading state;
- nothing is ever lost;
- no account and no cloud for Pluto itself, on the local network and offline; only the models' own
  traffic leaves, and an answer's safe images and link icons
  ([ADR-0022](adr/0022-the-triage-row-by-row.md));
- Linux first-rank: Wayland, sharp fractional scales, the HP smooth, Linux checked first at each
  release.

Also held, without a figure:
- WCAG 2.2 AA: a screen reader, the whole application by the keyboard, contrasts checked, reduced
  motion and transparency;
- no telemetry;
- bad input from agents, models or servers never crashes a client, the server or `pluto-agent`;
- secrets never in a log, an export, a capture or a model's context.

## 3. Decisions

### Fixed

| Topic | Decision |
|---|---|
| What Pluto is | A harness of its own; one kernel, two products: the work first, the assistant later ([ADR-0030](adr/0030-pluto-becomes-a-harness.md)); what still holds of [ADR-0003](adr/0003-what-pluto-is-to-become.md) |
| What the assistant runs on | API keys, DeepSeek, local models, and every plan a provider officially opens to third-party applications (ChatGPT's today); Claude by an API key until Anthropic offers an official sign-in billed as extra usage ([ADR-0041](adr/0041-the-assistant-on-every-plan-opened-to-third-parties.md)) |
| Engines | Pluto's loop; vendors' engines (Claude Code through a mod, Codex's app-server, Copilot's SDK); ACP for the others; the main work on Claude Code ([ADR-0031](adr/0031-three-kinds-of-engine.md)) |
| Pluto's loop | A small core, per-model profiles, four provider clients of its own, models.dev's registry, outside tools over MCP ([ADR-0032](adr/0032-plutos-loop-and-its-provider-clients.md)) |
| Sessions | One `pluto-agent` process each, an append-only log on disk; also a library ([ADR-0033](adr/0033-one-pluto-agent-process-per-session.md)); each in its own unit, never a child of the server or the command line, and an agent approves only what its brief grants ([ADR-0043](adr/0043-each-session-in-its-own-unit.md)) |
| The owner's method | Its mechanisms native (the delegation object, limits, end conditions, provenance, costs, fallback), the method in text ([ADR-0034](adr/0034-the-methods-mechanisms-native.md)) |
| Safety | Keyring, per-device keys, masking, no telemetry ([ADR-0009](adr/0009-secrets-privacy-and-the-sandbox.md)); the sandbox after Codex's, with a vault ([ADR-0035](adr/0035-the-sandbox-and-the-vault.md)) |
| The inspector | Shows every turn; appends by default; rewriting the past shows its price and asks ([ADR-0036](adr/0036-the-inspector-appends-by-default.md)) |
| How the harness is judged | Parity on frontier models for fewer tokens, a lead on local and cheap ones; Pluto's bench and a Terminal-Bench subset ([ADR-0037](adr/0037-how-the-harness-is-judged.md)) |
| The foundation | The same repository; new crates beside the frozen egui client; no gateway; the fork stops at Stage 5 ([ADR-0038](adr/0038-a-new-foundation-beside-the-fork.md)) |
| The order | The stages below ([ADR-0039](adr/0039-the-order-of-the-stages.md)) |
| Out of the plan | The editor, office editors, calls, the smart home: ideas ([ADR-0040](adr/0040-out-of-the-plan.md)) |
| The clients | Native Rust; egui until Stage 5, then uwui ([ADR-0001](adr/0001-a-native-client-on-egui-until-uwui.md)); the phone in Kotlin and Compose over the Rust core, then uwui ([ADR-0002](adr/0002-the-android-app-until-uwui.md)) |
| The protocol | Pluto's own, typed, versioned ([ADR-0005](adr/0005-plutos-own-typed-protocol.md)) |
| Machines | Several servers in one view, a relay on iroh ([ADR-0007](adr/0007-several-servers-and-a-relay.md)) |
| Storage | SQLite, versioned, indexing the sessions' logs; the history imported ([ADR-0008](adr/0008-storage-and-the-history-moved.md)) |
| Features | Today's, triaged; egui frozen ([ADR-0010](adr/0010-todays-features-triaged.md)); kept row by row, the specification accepted ([ADR-0022](adr/0022-the-triage-row-by-row.md), [ADR-0023](adr/0023-the-specification-accepted.md), [`specification.md`](specification.md)); the features of other harnesses' add-ons, built by Pluto, never installed ([ADR-0045](adr/0045-features-from-other-harnesses-add-ons.md)) |
| The market's features | Review in Pluto, triggers, one configuration for every agent, side questions, dictation, cloud runs, computer use in an isolated session; the phone's notifications through UnifiedPush, its own models ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md)) |
| Judged against the market | The best rival measured on each criterion sets the bar, beaten by a margin per family; the stricter of that and the budgets ([ADR-0017](adr/0017-pluto-measured-against-the-market.md)) |
| The chat | It replaces the vendors' applications; each vendor by its permitted path; one memory, local voice, one history ([ADR-0020](adr/0020-the-chat.md)) |
| uwui's decisions of 2026-11-07 | Agents reach uwui's applications only where the sandbox allows it; what only a person may do is never offered to agents; macOS decided when Pluto goes public ([ADR-0016](adr/0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md)) |
| License | MIT OR Apache-2.0 for new code ([ADR-0003](adr/0003-what-pluto-is-to-become.md)) |
| Platforms | Linux, Windows, macOS, Android and iOS; Windows, macOS and iOS at Stage 8, iOS as a client; the core portable from M2.0 ([ADR-0044](adr/0044-five-platforms-at-going-public.md)) |

### Open

Each is decided by an ADR when it becomes necessary, not earlier.

| Topic | Candidates | Decide at |
|---|---|---|
| The new foundation's architecture | the crates and their dependencies, the seams, how sessions survive the server, identifiers, the protocol's compatibility, ownership and limits, the sandbox's boundary; the list in [M2.0](PLAN.md#m20--the-new-foundations-architecture) | M2.0 |
| The public name | renaming later reaches crate and binary names, `~/.pluto` and the protocol's identifiers | M2.0, before a crate is published or a path fixed |
| How outside contributions are accepted | a Developer Certificate of Origin, or a contributor licence agreement | M2.0, before the first outside contribution |
| The interface's languages | English only, or texts kept apart from the code from the first screen so that French and others can follow | M2.0 |
| The way back | checkpoints of the files agents change; a trash for the assistant; how long each is kept | Stage 3, with the sandbox |
| Updates | no updater of Pluto's own beside the system's package manager; migrations with a tested recovery path; turns kept across an update | M2.0 |
| The session log's format | JSON Lines; a binary encoding; how entries, provenance and torn writes are handled | Stage 2, M2.1 |
| The bench's tasks and budget | how many saved briefs, from which projects, and what of them may sit in this public repository; the Terminal-Bench subset; runs per task; the monthly budget | Stage 2 |
| Whether Sign in with ChatGPT covers Pluto's loop | a first test on the owner's account; OpenAI's interest form | Stage 3, the provider clients' milestone |
| The budgets of `pluto-agent` and its command line | from their first figures | Stage 3, its first milestone |
| The TUI's budgets and rivals | Claude Code's, Codex's and OpenCode's TUIs | Stage 7 |
| A keyring without a graphical session | systemd's credentials; a file sealed by the session's keyring; another | Stage 3, the vault's milestone |
| DeepSeek Harness beside DeepSeek's models in Pluto's loop | an ACP engine, or none; the fork drives its local Web protocol meanwhile ([ADR-0021](adr/0021-deepseek-harness-in-todays-pluto.md)) | Stage 4 |
| The protocol's wire format and transports | JSON over WebSocket; a binary encoding (CBOR, postcard); QUIC through iroh away from home | Stage 5, the protocol's milestone |
| A thread moved between machines | how, and whether | after Stage 5 |
| Agents' sessions kept ready for the chat | how many, and for how long, against "nothing runs at rest" | Stage 6, measured |
| Gemini through its official CLI, or a sign-in Google offers to third-party applications | allowed or not by Google, which has not answered; an API key until then ([ADR-0041](adr/0041-the-assistant-on-every-plan-opened-to-third-parties.md)) | when Google answers |
| Tuta's calendar | TutaBridge serves only mail; another bridge, or none | Stage 7 |
| Where the assistant runs once it leaves the desktop | a NAS, or a server | Stage 7 |
| Signal and Matrix as the assistant's channels | an open path, not planned | after Stage 7 |
| Pluto as the owner's assistant on the desktop, calling the actions uwui's applications declare, where Linux has no such mechanism | built or not, and how | Stage 7, once uwui's local MCP server exists |
| Extensions beyond text files | WebAssembly components, MCP servers; only with a second concrete use | the owner's go |
| Going public | each provider's terms, kept in the table of the vendors' paths ([ADR-0020](adr/0020-the-chat.md)); Flatpak, AppImage, Windows and macOS packages, iOS through the App Store; the server's sandbox, keyring, sessions' units and terminals ported to Windows and macOS ([ADR-0044](adr/0044-five-platforms-at-going-public.md)); the sandbox on Android | Stage 8 |

## 4. Stages

Stage 1, the ground measured and triaged, is in [`history.md`](history.md): its milestones are done
or moved, and it closes with its stage audit.

### Stage 2 — The bench and the session log

**Goal.** Pluto can measure any harness on the owner's own work and on a public benchmark, and
every session from Stage 3 on has a format to write.

- The new foundation's architecture: its crates and the direction of their dependencies, the
  seams as contracts, a way in for contributors, checked in CI.
- The session log's format, in a crate of its own, versioned.
- Pluto's bench: saved briefs replayed with their hidden checks, in a sandbox, graded outside the
  agent's reach.
- A subset of Terminal-Bench through the same runner.
- The vendors' harnesses measured on both, as the reference Pluto's loop must reach.

**Exit.** The bench runs in CI on the fake model, and by hand on real models. The vendors' harnesses
have figures with their confidence intervals. Recorded sessions convert into the log.

### Stage 3 — `pluto-agent` and its command line

**Goal.** The owner's delegated tasks run on Pluto's own loop, started and followed from a command
line ([ADR-0042](adr/0042-a-cli-first-the-tui-with-the-assistant.md)).

- Pluto's loop and its provider clients: Anthropic's Messages, OpenAI's Responses, Chat
  Completions, Ollama's native API; the model registry and the quirks table.
- `pluto-agent`, one process per session, writing its log.
- The sandbox and the vault.
- The delegation object, its limits and end conditions; the owner's method read from its files.
- The command line: run a brief, list and follow sessions, answer approvals, show the inspector's
  view; the same by SSH, from a script and from an agent.
- Its features taken from other harnesses' add-ons, in the
  [specification §8](specification.md#8-features-from-other-harnesses-add-ons)
  ([ADR-0045](adr/0045-features-from-other-harnesses-add-ons.md)).

**Exit.** The owner's delegated tasks run through `pluto-agent` every day. The loop beats the
OpenCode companion on the bench on the owner's local models, and replaces it.

### Stage 4 — The engines

**Goal.** Claude Code, Codex, Copilot and the ACP agents run under Pluto's thread model, with
their quotas and exact costs.

- Claude Code through Pluto's mod; Codex through its app-server; Copilot through its SDK; ACP.
- Quotas and plan limits from each engine; exact costs; providers down, fallback and resume.
- The recordings Stage 1 left, made with these engines, and their replay tests.
- Cross-agent review, comparison of several models, delegation between providers.
- Its features taken from other harnesses' add-ons, in the
  [specification §8](specification.md#8-features-from-other-harnesses-add-ons)
  ([ADR-0045](adr/0045-features-from-other-harnesses-add-ons.md)).

**Exit.** The owner's main work runs on Claude Code under Pluto, and every first-rank engine passes
the grid of recorded sessions.

### Stage 5 — The server, and the fork stopped

**Goal.** The fork's Node server no longer runs, and every client speaks Pluto's protocol
([ADR-0038](adr/0038-a-new-foundation-beside-the-fork.md)).

- Pluto's server, supervising the sessions; the protocol's crate; pairing.
- The scheduler and triggers; the owner's inbox; the browser through CDP; computer use in an
  isolated session.
- Several servers in one view, and the relay on iroh
  ([ADR-0007](adr/0007-several-servers-and-a-relay.md)).
- The base of the uwui client: threads, the timeline, the composer.
- The Android app on Pluto's protocol.
- The history imported and verified; the fork stopped; the egui client removed.
- The rivals' campaigns Stage 1 left.
- Its features taken from other harnesses' add-ons, in the
  [specification §8](specification.md#8-features-from-other-harnesses-add-ons)
  ([ADR-0045](adr/0045-features-from-other-harnesses-add-ons.md)).

**Exit.** The fork is stopped and the egui client gone. The budgets are met on the high tier and
measured on the HP.

### Stage 6 — The uwui client completed

**Goal.** The desktop client holds every space the specification keeps.

- The chat, its memory and projects ([ADR-0020](adr/0020-the-chat.md)).
- Local models managed from Pluto: installed, pulled by name, their GPU memory shown
  ([`specification.md`](specification.md)).
- The dashboard, with resources per agent and per machine.
- Design: mockups, design systems as files, graphics, a canvas; visual editing of the real running
  app or mockup.
- Deep research with replayable sources; publishing a page; tools' own interfaces in uwui's
  isolated web view.
- WCAG 2.2 AA; then the web through uwui's web target.
- Its features taken from other harnesses' add-ons, in the
  [specification §8](specification.md#8-features-from-other-harnesses-add-ons)
  ([ADR-0045](adr/0045-features-from-other-harnesses-add-ons.md)).

**Exit.** Every budget is met on the high tier and on the HP, and the owner works in Pluto only.

### Stage 7 — The personal assistant

**Goal.** The second product, on the same kernel, in its own daemon
([ADR-0030](adr/0030-pluto-becomes-a-harness.md)).

- Its own system user, vault, budget and rights; approvals checked outside the model.
- A morning brief and monitoring; mail through IMAP, Tuta's through TutaBridge; calendar and
  lists; a second brain of notes as files; the machine and the NAS.
- Watchers as scripts; a model called only on a signal.
- Full voice conversation, local.
- A light TUI, for SSH and machines without a screen: threads, following, approvals, the
  inspector ([ADR-0042](adr/0042-a-cli-first-the-tui-with-the-assistant.md)).
- Its features taken from other harnesses' add-ons, in the
  [specification §8](specification.md#8-features-from-other-harnesses-add-ons)
  ([ADR-0045](adr/0045-features-from-other-harnesses-add-ons.md)).

**Exit.** The assistant runs every day on the owner's desktop within its budget, with no action
taken without its approval.

### Stage 8 — Going public

**Goal.** Pluto opens to others, on the owner's go.

- The open decisions of going public taken.
- Windows and macOS, client and server; iOS, the client ([ADR-0044](adr/0044-five-platforms-at-going-public.md)).
- Packages beyond Nix.
- A release audit.

**Exit.** A public release, after the owner's go.

## 5. Release map

| Version | Stage | Content |
|---|---|---|
| 0.2 | 2 | The bench and the session log |
| 0.3 | 3 | `pluto-agent` and its command line |
| 0.4 | 4 | The engines |
| 0.5 | 5 | Pluto's server, the fork stopped |
| 0.6 | 6 | The uwui client completed |
| 0.7 | 7 | The personal assistant |
| 1.0 | 8 | Public |

Releases before 1.0 are private: a version, a CHANGELOG and a tag, after the owner's go.
