# 0042. A command line at Stage 3, a light TUI with the assistant

- Status: Accepted; amends [0030](0030-pluto-becomes-a-harness.md) (the TUI's place among the
  surfaces), [0033](0033-one-pluto-agent-process-per-session.md) (what supervises sessions before
  the server) and [0039](0039-the-order-of-the-stages.md) (what Stages 3 and 7 deliver)
- Date: 2026-11-15

## Context

The design session of 2026-11-15 listed three surfaces: the uwui desktop client, a complete TUI, a
phone that does everything ([ADR-0030](0030-pluto-becomes-a-harness.md)). Stage 3 was to bring
`pluto-agent` with its TUI, which would follow threads, the delegation and the inspector
([ADR-0039](0039-the-order-of-the-stages.md)). A TUI first made sense while uwui's date was unknown.

The same day the owner said that uwui will be ready before Stage 3, then questioned the TUI's use
beyond SSH:
- A complete TUI doubles the uwui client for code, chat, the delegation and the inspector, at the
  same bar, and cannot hold the graphical spaces: design, the canvas, visual editing.
- The owner works at a desktop, and reaches other machines through several servers in one view,
  the relay and the phone ([ADR-0007](0007-several-servers-and-a-relay.md)).
- The owner's delegated tasks, Stage 3's first slice of work, are started by a lead agent, not by
  hand: they need commands a script or an agent can call.
- A terminal is what remains on a machine without a screen: a NAS or a server, where the
  assistant may run once it leaves the desktop.

## Decision

**Stage 3 brings a command line, not a TUI.** `pluto-agent`'s commands run a brief, list and follow
sessions (the log printed as text), answer approvals and show the inspector's view of the context.
They work the same by SSH, from a script and from an agent, and are tested without a screen.

**A light TUI comes with the assistant, at Stage 7**, for SSH and machines without a screen:
threads, following, approvals and the inspector, without the graphical spaces.

**Until the server exists, the command line supervises the sessions** it starts.

## Alternatives considered

- **The complete TUI at Stage 3**, as first decided. A terminal surface from the start. But a second
  whole interface to hold at the bar, beside a uwui client that is ready.
- **The TUI only as an idea**, built if a need shows. Less to plan. But the assistant on a NAS or a
  server is a known need.
- **The uwui client from Stage 3**, over `pluto-agent` as a library. A graphical Pluto sooner. But it
  reopens the decision of the same day that puts its base at Stage 5 with the server, and its
  connection would be redone there.

## Consequences

- The egui client stays the owner's graphical surface until the uwui client's base, at Stage 5.
- The budgets of `pluto-agent` and its command line are set at Stage 3 from their first figures;
  the TUI's at Stage 7.
- The TUI's rivals (Claude Code's, Codex's and OpenCode's) are measured with it, at Stage 7.
