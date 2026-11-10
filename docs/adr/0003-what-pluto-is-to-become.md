# 0003. What Pluto is to become

- Status: Accepted; amended by [0011](0011-plutos-own-harness-minimal-and-measured.md) (Pluto's own harness) and
  [0016](0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md) (the look, the bar and macOS, after uwui's decisions),
  [0018](0018-the-markets-features-and-the-triage.md) (review in Pluto, the market's features
  added), [0019](0019-a-code-editor-in-pluto.md) (a code editor in Pluto; going public at Stage 8)
  and [0020](0020-the-chat.md) (the chat replaces the vendors' applications) and
  [0022](0022-the-triage-row-by-row.md) (the dashboard as the home screen; glass not a setting;
  safe images and icons of answers loaded)
- Date: 2026-11-06

## Context

Until 2026-11-06, Pluto aimed at parity with T3 Code v0.1.0. It was a native client in Rust that
reproduced T3's web client ([ADR-0001](0001-a-native-client-on-egui-until-uwui.md)) and spoke to a
TypeScript fork of T3's server, `pluto-server`. The fork adds native drivers for Kimi Code, Mistral
Vibe, Z Code and Ollama, a chat space with memory, and design projects. The owner uses Pluto all
day.

On 2026-11-06 the owner changed direction, in three ways:
- Pluto is to be truly different from T3 Code.
- It may get a server of its own.
- It becomes one of the applications of reference of uwui, the owner's UI framework, which its
  client will move to.

The owner answered an interview that day, one theme at a time, after a survey of the field. The
products surveyed were T3 Code, the Codex app, Claude's desktop app, Cursor, Conductor, Zed with
its agents, OpenCode's desktop and others.

The survey found five things, as of 2026-11-06:
- Every rival but Zed is built on Electron. Users report memory growth into gigabytes: the Codex
  app at 1.3 to 2.2 GB at rest, Zed's agent panel growing from 0.3 to 3 GB.
- None treats local models as equals. T3 Code refuses them.
- None shows every provider's quota in one place.
- On Linux, most are in preview or crash.
- Review, not parallelism, is the bottleneck users name.

This ADR records what Pluto is to become. The decisions that need their own record follow it:
- [0004](0004-plutos-own-server-in-rust.md): the server;
- [0005](0005-plutos-own-typed-protocol.md): the protocol;
- [0006](0006-agents-drivers.md): the drivers;
- [0007](0007-several-servers-and-a-relay.md): several machines;
- [0008](0008-storage-and-the-history-moved.md): storage;
- [0009](0009-secrets-privacy-and-the-sandbox.md): secrets and safety;
- [0010](0010-todays-features-triaged.md): the feature set.

## Decision

**What makes Pluto different.** Four things, together:
- One place for every agent and every subscription.
- Native and frugal: it starts at once, holds little memory, and does nothing at rest.
- Code and chat in one application.
- Private and local first.

T3 Code stops being the goal. It becomes one reference among others.

**For whom.** For the owner first. Pluto is designed so it can go public later: no machine's paths
written in, versioned formats, a license that allows it. The repository stays private until the
owner decides otherwise.

**How it is used.** All four ways of working, every day:
- several agents in parallel;
- one agent followed closely and steered;
- long tasks that run alone, scheduled or overnight;
- chat.

A busy day holds 5 to 15 live threads. Everything in use today is used weekly: code threads and
diffs, the terminal and the browser, the chat space with its memory, design projects, scheduled
tasks and the phone. What weighs today is too many features and heaviness.

**Agents and models.** Every agent Pluto drives is first-rank:
- Claude Code and Codex;
- OpenCode, Kimi Code, Mistral Vibe and Z Code;
- Grok, Antigravity, Pi and Muse;
- DeepSeek Harness;
- local models through Ollama.

Cursor is dropped: the owner does not use it. Local models are equals of the others: the same
picker, chat and code, a thread moved from one model to another, and models managed from Pluto
(pulled, deleted, their memory on the GPU shown). Quotas are shown in one place, with a warning
before one runs out and a suggested switch to another agent. The switch is never made alone.
Agents work together in three ways:
- one reviews another's work, from another provider;
- one request goes to several models, their answers side by side;
- one agent delegates a subtask to another provider's agent.

**The server.** Pluto gets its own server, in Rust, built step by step in front of the frozen fork
([ADR-0004](0004-plutos-own-server-in-rust.md)), with a typed protocol of its own
([ADR-0005](0005-plutos-own-typed-protocol.md)). It is built first, while uwui matures. The client
on uwui comes after, and speaks the new protocol from its start.

**Machines, phone and web.** Each machine that codes runs a server, and each client shows them all
in one view ([ADR-0007](0007-several-servers-and-a-relay.md)). Away from home, Pluto goes through a
relay of its own, end-to-end encrypted, that the owner hosts. The Android app stays in Kotlin until
uwui runs whole Android applications, then shares the desktop's interface code. A web client comes
later, through uwui's web target. The server is a service: closing the window stops nothing, and a
tray icon and notifications stay.

**Tools.**
- **Terminal**: full, as today.
- **Files**: viewed and lightly edited, and commented for the agent. Real editing happens in the
  owner's editor, which Pluto opens at the line.
- **Git**: a worktree per thread, commit and push in one action, and pull requests followed as
  links with their state. Review happens on the forge.
- **Browser**: runs on the server, driven by agents through CDP, and watched by the owner in a tab.
- **Dashboard**: shows the live agents and their state, the quotas and usage, the machines and
  their resources, and the scheduled tasks. It replaces the idea of a review queue.

**Data and privacy.** The server stores in SQLite, its schema versioned and migrated, with a
readable export of any thread. Today's history moves from the fork, verified
([ADR-0008](0008-storage-and-the-history-moved.md)). Nothing leaves the owner's machines but the
agents' own traffic: no telemetry, crash reports kept on the machine, no update check without
consent.

**Secrets and safety.** Secrets live in the system's keyring, and each paired device has its own
revocable key. Agents run in a sandbox that can be adjusted per project or thread, locked by
default for unattended tasks. Secrets that agents meet are masked on screen, in exports and in
notifications ([ADR-0009](0009-secrets-privacy-and-the-sandbox.md)).

**Performance and machines.**
- **The high tier**: the owner's desktop, 4K at 240 Hz.
- **The lowest tier**: the owner's HP laptop, an Athlon Silver 3050U with 4 GB, the machine uwui
  measures first. It runs the client and a server with two or three agents, smoothly, without
  swap.
- **The budgets**: [`ROADMAP.md`](../ROADMAP.md#quality-bar).

**The interface.** Pluto's own look on uwui: uwui's design system, Geist and Lucide, and a theme
and a density of Pluto's own. Glass is an option, and the imitation of T3 ends. Rich motion on the
high tier, lightened on the HP and under reduced motion, and nothing moving at rest. Keyboard
first: everything can be done from the keyboard, the shortcuts are remappable, and the mouse stays
complete. Accessibility at uwui's level: WCAG 2.2 AA, checked.

**Distribution and license.** Linux first, as a Nix package. Flatpak, AppImage and Windows, the
server included, come when Pluto goes public, checked in the owner's Windows virtual machine. Code
is written from the start with nothing tied to Linux. Android ships as an APK. The new code is
under MIT OR Apache-2.0, like uwui, which fits the MIT code taken from T3.

**The future kept in view.** Nothing here is built without the owner's go:
- an agent loop of Pluto's own, for local models and any API, in place of the OpenCode companion;
- local voice dictation;
- isolated extensions in WebAssembly;
- design projects with OpenDesign's catalog.

**The feature set.** Today's features are inventoried and triaged with the owner (keep, cut or
rethink), and what is kept, plus the differences above, becomes the specification of the server
and of the uwui client. The egui client is frozen meanwhile
([ADR-0010](0010-todays-features-triaged.md)).

**Core or optional.** Every need above is either in Pluto's core, always present, or optional:
present only when the owner turns it on or installs what it needs. The split is chosen for the long
term.

| Need | Core or optional | Why |
|---|---|---|
| Threads with every first-rank agent | Core | What Pluto is |
| Local models as equals | Core; Ollama installed by the user | A difference; Ollama stays an outside program |
| Chat space, its memory and projects | Core | Code and chat in one application |
| Design projects | Optional, a kind of project | Not every project is a design |
| Dashboard | Core | Following 5 to 15 threads |
| Quotas with a suggested switch | Core | A difference |
| Cross-agent review, comparison, delegation | Core, used by choice | Built into orchestration; never automatic without the owner's setting |
| Scheduled tasks | Core | Long tasks that run alone |
| Terminal, diffs, light editing, worktrees | Core | Daily work |
| Browser on the server | Optional: present where Chromium is installed | A heavy outside program |
| Several servers in one view | Core | The model of every connection |
| Relay | Optional, a binary of its own | Needed away from home only; a private network works without it |
| Android app | Optional, an app of its own | A second product over the same core |
| Web client | Optional, later | uwui's web target |
| Sandbox, secret masking, keyring | Core | Cannot be added safely later |
| Notifications and tray | Core | The server works while the window is closed |
| Readable export | Core | The owner's data stays the owner's |
| Accessibility | Core | Cannot be added safely later |
| Pluto's own agent loop | Core once built | Local models without another program |
| Voice dictation | Optional | Needs a speech model downloaded |
| Extensions | Optional | After uwui's own extensions exist |
| OpenDesign's catalog | Optional, downloaded on demand | Outside content with its own licenses |

## Alternatives considered

- **Keep parity with T3 Code.** Fewer decisions to take. But the owner no longer wants Pluto to be
  T3. And following a project that made 3,389 commits in October 2026 is beyond one person.
- **A blank page instead of a triage.** The freest design. But it risks forgetting what serves the
  owner without their thinking of it.
- **A review queue instead of a dashboard.** Review is the field's bottleneck. The owner chose an
  overview of everything first.
- **Automatic switching on quotas.** Nothing would ever stop. But a weaker agent could take over
  delicate work unseen.
- **Pluto for the owner alone, or public soon.** The first would close the door. The second would
  bring support and the question of third-party CLIs and their terms too early.

## Consequences

- AGENTS.md drops the parity goal and the rule against porting the server to Rust.
  `docs/release-backlog.md` is abandoned. The private 1.0 planned on 2026-10-28 no longer exists.
- `docs/parity/` stays as the inventory that the triage starts from.
- [`uwui-needs.md`](../uwui-needs.md) lists what Pluto asks of uwui, for SlimeEngine's M6d.1b3.
- The differences reach the screen with the uwui client. The server's share of them comes before.
- Going public waits on decisions left open in [`ROADMAP.md`](../ROADMAP.md#open):
  - the terms of each provider. Anthropic does not allow third-party products to offer claude.ai
    sign-in without approval;
  - the name;
  - the platforms.
