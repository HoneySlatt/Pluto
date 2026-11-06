# 0004. Pluto's own server, in Rust, in front of the frozen fork

- Status: Accepted
- Date: 2026-11-06

## Context

Pluto's server is `pluto-server`, a fork of T3 Code v0.1.0's TypeScript server (MIT), at
`~/Projects/pluto-server`, branch `pluto`. On 2026-10-27 a Rust server was rejected. The reasons
were that the server was about 480,000 lines, mostly waiting on I/O, and that following upstream
seemed possible. Measured on 2026-11-06:

- **Size.** 373,000 lines of server-side TypeScript, 118,000 of them drivers. The client uses about
  76 of its 183 methods.
- **Upstream's pace.** Upstream made 3,389 commits in October 2026, and 2,323 in the 12 days after
  v0.1.0.
- **The fork has never merged upstream.** 69 of its 176 changed files changed upstream too.
- **Memory.** The Node server holds 514 MB under load. Its five `claude` processes beside it hold
  1,520 MB, which no server language changes.
- **Paths for Rust.** Every driver the owner uses has one: a CLI or server protocol, often with a
  Rust crate. Cursor's agent exists only as a TypeScript library, and the owner does not use it.
  For Claude, Anthropic's documentation says to run the CLI as a subprocess from other languages
  (`-p`, `stream-json`, approvals through an MCP tool, SIGINT, `--resume`). A few controls in the
  middle of a session exist only in the SDKs' internal channel.

Estimates, ±50 %:
- following upstream costs 4 to 8 days a month, and growing;
- a full Rust server costs 30 to 55 weeks of AI-assisted work;
- the triage ([ADR-0010](0010-todays-features-triaged.md)) is what shrinks that figure most.

## Decision

**Pluto gets its own server, in Rust, with no Node.** It lives in this repository's workspace,
beside the protocol crate and the clients.

**The fork is frozen.** It no longer follows upstream. It receives only the fixes that keep the
owner working, until it stops.

**The Rust server stands in front of the fork.** Every client speaks only Pluto's protocol
([ADR-0005](0005-plutos-own-typed-protocol.md)) to the Rust server. Threads whose agent is not yet
ported are forwarded to the fork behind it and translated. The fork's protocol client, written in
Rust today in `crates/pluto-core`, moves into the server for that. Each agent ported shrinks what
goes to the fork. When none goes there, the history is imported
([ADR-0008](0008-storage-and-the-history-moved.md)) and the fork stops.

**Claude is the first driver**, the owner's choice. An agent speaking ACP is built in the same
milestone, so that the driver interface does not take the shape of one agent.

**It is built step by step, each step in daily use.** The order is in
[`ROADMAP.md`](../ROADMAP.md#4-stages). The server keeps only what the triage keeps.

## Alternatives considered

- **Keep following upstream**: upstream's features for free, but already 2,300 commits behind and
  beyond one person.
- **Freeze the fork and own it**: nothing to build, but 373,000 lines of another team's TypeScript
  on Effect 4, still a release candidate, and the weight that the owner names as a pain.
- **A Rust server beside the fork, each client speaking both**: the Rust server stays pure. But the
  shared core, under egui, Android and uwui, would carry two protocols for the whole transition.
  The owner first chose it, then chose this ADR's way once uwui's client was known to come during
  the transition.
- **A Rust server built whole, then switched in one go**: no coexistence, but nothing useful for
  months.
- **A small Node module for TypeScript-only SDKs**: it is needed only by Cursor, which is dropped.
  DeepSeek Harness is decided when its interface is known ([ADR-0006](0006-agents-drivers.md)).

## Consequences

- AGENTS.md's rule "do not port the server to Rust" ends.
- Until the fork stops, Node runs behind the Rust server. On the HP laptop, no budget can be met
  during that time. The owner's desktop carries the transition.
- The translation from the fork's protocol to Pluto's is code that is thrown away when the fork
  stops.
- Claude's internal controls are covered by tests that replay recorded sessions, so that an update
  of `claude` that breaks one shows in CI.
