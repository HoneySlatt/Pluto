# 0038. A new foundation beside the frozen fork, in the same repository

- Status: Accepted; supersedes [0004](0004-plutos-own-server-in-rust.md); amends
  [0001](0001-a-native-client-on-egui-until-uwui.md) and [0010](0010-todays-features-triaged.md)
  (the egui client ends with Stage 5, never moved to Pluto's protocol) and
  [0002](0002-the-android-app-until-uwui.md) (the Android app reaches Pluto's protocol through the
  new foundation)
- Date: 2026-11-15

## Context

[ADR-0004](0004-plutos-own-server-in-rust.md) put a Rust server in front of the frozen fork:
clients would speak Pluto's protocol to it, and threads of agents not yet ported would be forwarded
to the fork and translated. Its own consequences: Node keeps running behind it, and the
translation is code thrown away when the fork stops.

Since 2026-11-15 Pluto is built around a harness ([ADR-0030](0030-pluto-becomes-a-harness.md)), and
the work moves onto it in an order where the server comes at Stage 5
([ADR-0039](0039-the-order-of-the-stages.md)). Today's code is the frozen egui client, the shared
core `crates/pluto-core` (which speaks T3's protocol), the Android app, and the crates of Stage 1's
measures and recordings.

On 2026-11-15 the owner said that uwui, the owner's UI framework, will be ready before Stage 3.

## Decision

**The same repository, a new foundation.** The new crates start from zero beside the frozen egui
client and `pluto-core`. Code of today's crates is copied into them only where it fits the new
model; the old crates stay frozen with what uses them.

**No gateway.** Nothing translates the fork's protocol. The fork keeps serving the egui client and
the Android app, unchanged, until the end of Stage 5.

**The work moves slice by slice.** From Stage 3, each milestone names the slice of the owner's daily
work it moves onto the new foundation. The owner's delegated tasks move first.

**Stage 5 ends the fork.** With Pluto's server and protocol come the base of the uwui client
(threads, timeline, composer) and the Android app on them. The fork's history is imported,
resumably. Then the fork stops, and the egui client is removed. It is never moved to Pluto's
protocol.

**The lead's implementation choices**, from the challenge, shown to the owner with this record:
- the history is imported thread by thread, as each one finishes;
- the Android app reaches Pluto's protocol through the new foundation's crates, wrapped for Kotlin
  as today; `pluto-core` and `pluto-ffi` leave with the egui client.

## Alternatives considered

- **ADR-0004's gateway.** Every client on Pluto's protocol early. But translation code thrown away,
  and the new server shaped around the fork's threads rather than the harness's.
- **A new repository.** A clean start. But the decisions, measures and recordings would be split in
  two places.
- **The egui client moved to Pluto's protocol at Stage 5.** The fork could stop without uwui. But
  with uwui ready before Stage 3, that work would be thrown away one stage later.
- **The fork kept until the whole uwui client, in Stage 6.** Nothing to change in Stage 5's
  clients. But Node runs one stage longer, and the phone moves before the desktop.

## Consequences

- AGENTS.md's rule changes: new code never speaks T3's protocol; the egui client and the Android
  app speak it until the end of Stage 5.
- Rust without Node holds for the new foundation, with the one exception of
  [ADR-0031](0031-three-kinds-of-engine.md).
- Until the fork stops, Node runs, and the HP's budgets cannot be met; the owner's desktop carries
  the transition, as before.
- The base of the uwui client comes at Stage 5, before the chat, the dashboard and design of
  Stage 6.
- The recordings of Stage 1 replay into the fork today; from Stage 4 they also test the Claude Code
  engine and an ACP agent.
