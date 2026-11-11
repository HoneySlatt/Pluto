# 0015. The budgets set from today's figures

- Status: Accepted; amended by [0017](0017-pluto-measured-against-the-market.md) (rivals may
  tighten each budget, never loosen it; frames at 99.9 %; input timed to the photon) and [0026](0026-the-rivals-measured-how.md) (memory counted as private,
  swap and GPU memory)
- Date: 2026-11-07

## Context

The quality bar of the [ROADMAP](../ROADMAP.md#quality-bar) left six budgets without a figure, to be
set from the measurements of [M1.2](../PLAN.md#m12--today-measured). Those measurements are in
[`measurements.md`](../measurements.md). Four of the six cannot be measured on today's code: the
client does not ask for the compositor's presentation times, the fixture runs no turn, and an
agent's resume needs a real agent.

## Decision

The owner validated these figures on 2026-11-07:

**Input to present**: within one frame at p99, as uwui's own budget. Measured from Stage 6, when
the client runs on uwui.

**The binary's size**: at most 30 MB stripped for each binary, the client and the server. Today's
client is 59.5 MB stripped.

**An event's latency, server to client**: p99 within 5 ms on loopback, within 16 ms, one frame at
60 Hz, on the local network. Measured by the protocol's tests from Stage 2.

**The history's import**: at most 60 seconds on the high tier and 5 minutes on the HP, and
resumable if interrupted. Today's history is 2.79 GiB; the final states that
[ADR-0008](0008-storage-and-the-history-moved.md) imports are 927 MiB of it.

**An idle agent's resume**: its first event within 2 seconds of the message sent, on the high tier,
measured by the owner from Stage 2.

**The database's growth**: at most 25 MiB a day of heavy use, ten times less than today's 240 MiB.
Today's database writes every update of an item in full, then keeps its last state again in its
projections; Pluto's storage keeps each final state once, drops superseded states, and stores long
tool outputs once, compressed.

## Alternatives considered

- **Today's figures as the budgets**: the client's 59.5 MB and the database's 240 MiB a day would
  be met at once, and would ask nothing of the new code.
- **Waiting for the HP's figures**: they bear on the HP's own budgets, not on these six.
- **No budget for what cannot be measured yet**: the targets would then be set by the code that
  meets them, the reverse of the method.

## Consequences

- The ROADMAP's quality bar holds them, each with what measures it and when.
- Four are checked only once Stage 2, 3 or 6 builds what they measure.
- The database's budget asks Stage 2's storage to keep each item's final state once
  ([ADR-0008](0008-storage-and-the-history-moved.md)'s open choice between an event log and plain
  tables must meet it).
