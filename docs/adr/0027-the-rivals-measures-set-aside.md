# 0027. The rivals' measures set aside until the owner's go

- Status: Accepted; amends [0017](0017-pluto-measured-against-the-market.md) (Stage 2 no longer
  waits for the rivals' figures)
- Date: 2026-11-11

## Context

[ADR-0017](0017-pluto-measured-against-the-market.md) asked that a milestone of Stage 1 measure
the rivals, so that Stage 2's server starts from their figures, and Stage 1's exit in the
[ROADMAP](../ROADMAP.md) wants the rivals' figures beside today's. [M1.6a](../PLAN.md#m16a--the-fake-model-and-the-rivals-bench)
built the fake model and the rivals' bench ([ADR-0026](0026-the-rivals-measured-how.md)) and took
first figures of Pluto, T3 Code and Zed at rest; a full campaign takes hours of the owner's
machine, and M1.6b and M1.6c remain. On 2026-11-11 the owner stopped the measures to move on.

## Decision

The owner's decision of 2026-11-11.

**The rivals' measures wait for the owner's go.** M1.6b and M1.6c, and what M1.6a moved to M1.6b,
are set aside; no session starts them without the owner's word. The fake model, `pluto-measure`
and the bench stay as they are, ready.

**Stage 1 ends without them.** Its exit no longer waits for the rivals' figures; those M1.6a took
stay in [`measurements.md`](../measurements.md) as first figures.

**Stage 2 starts from the budgets.** Until the rivals are measured, the quality bar is the
ROADMAP's budgets alone; the rivals tighten them when their figures come, as ADR-0017 says.

## Alternatives considered

- **Finish M1.6 first, as ADR-0017 planned**: Stage 2's server built against the market's figures,
  but hours of measuring before any new work, which the owner did not want now.
- **Measure only the stream scenario, then stop**: a few figures more, still hours of the machine.

## Consequences

- Stage 2's server may be built looser than the best rival on some criterion until the rivals are
  measured; the budgets already set bound it meanwhile.
- The rivals are measured later, at their versions of that day; the packages downloaded on
  2026-11-11 will then be old, and downloaded again under the owner's go of ADR-0026.
- The fake model also serves Stage 2: its token budget and the chat's delay use it.
