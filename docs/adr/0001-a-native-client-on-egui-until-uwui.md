# 0001. A native client, on egui until uwui

- Status: Accepted; amended by [0016](0016-uwuis-decisions-of-2026-11-07-taken-by-pluto.md) (Pluto depends on uwui from its own repository)
- Date: 2026-11-06, recording a decision of 2026-10-20

## Context

Pluto began on 2026-10-20 as a native client for T3 Code's server, in place of T3's Electron and
web client. The owner wanted a program that starts at once, holds little memory and draws nothing
at rest. egui was chosen because it was ready and draws everything itself on the GPU. Today the
client holds about 110,000 lines in `src/`, and `crates/pluto-core` holds the connection, the
model and the timeline, which the Android app shares
([ADR-0002](0002-the-android-app-until-uwui.md)).

uwui is the owner's UI framework, built in SlimeEngine's Stage 6d and then in its own repository.
Pluto's client is one of the programs of uwui's first stage (SlimeEngine's ADR-0203). uwui's API
is not declared stable before it runs Pluto's client at the bar set in
[ADR-0003](0003-what-pluto-is-to-become.md).

## Decision

**The client is native Rust, drawn by its own framework, never a web view.** It stays on egui until
uwui can carry it, then moves to uwui. The move is a rewrite of the interface, written from the
triaged specification ([ADR-0010](0010-todays-features-triaged.md)), on Pluto's protocol
([ADR-0005](0005-plutos-own-typed-protocol.md)). It is not a port of the egui code.

**When it moves** is decided with uwui's plan, once uwui is ready to take its pilots. What Pluto
asks of uwui is in [`uwui-needs.md`](../uwui-needs.md).

## Alternatives considered

- **A web view (Tauri) or Electron**: the web client for free, but the memory and start-up that
  every rival suffers from, which is what Pluto exists to avoid.
- **GPUI, Iced or Slint**: frameworks that are ready today. But the owner's applications converge
  on uwui, and Pluto is one of its pilots.
- **Staying on egui for good**: no rewrite. But egui redraws every frame something moves, has none
  of uwui's motion, glass or accessibility checks, and holds Pluto outside the owner's framework.

## Consequences

- The egui client is frozen meanwhile, except fixes, the triage's cuts and the move to Pluto's
  protocol ([ADR-0010](0010-todays-features-triaged.md)).
- Pluto's needs weigh on uwui's first stage: its Markdown, terminal, code and diff views, rich text
  and streaming text are needed there, not after uwui's 1.0.
