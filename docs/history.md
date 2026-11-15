# History

The stages that are done, each in a few lines, with its release and its ADRs. The current stage is
in [`PLAN.md`](PLAN.md); the detail of each milestone is in the git history of `docs/PLAN.md`.

## Stage 1 — The ground measured and triaged

2026-11-06 to 2026-11-15. No release. ADRs [0003](adr/0003-what-pluto-is-to-become.md) to
[0029](adr/0029-the-benchs-tools-and-what-a-rivals-figure-holds.md). The owner decided on 2026-11-15
to close it ([ADR-0039](adr/0039-the-order-of-the-stages.md)): its milestones are done or moved
to later stages, and it closes with its stage audit, in `docs/audits/`, and the update of its
dependencies.

Pluto came under the owner's method, changed direction away from T3 Code, measured itself and the
fork, triaged every feature with the owner, cut the client and the phone, recorded agents'
sessions, and built a fake model and a bench to measure its rivals. On 2026-11-15 a design session
made Pluto a harness ([ADR-0030](adr/0030-pluto-becomes-a-harness.md)).

### M1.1 — Under the method

`scripts/check.sh` and CI ([ADR-0012](adr/0012-the-checks-and-ci.md)); `docs/ARCHITECTURE.md`.
Done 2026-11-07.

### M1.2 — Today measured

The client, the fork and its database measured on the fixture, in
[`measurements.md`](measurements.md) ([ADR-0014](adr/0014-how-today-is-measured.md)); six budgets
set ([ADR-0015](adr/0015-the-budgets-set-from-todays-figures.md)). Done 2026-11-07; the HP's
figures wait for the owner's hand.

### M1.3 — The triage

Every feature in [`features.md`](features.md), marked by the owner; the
[specification](specification.md) accepted ([ADR-0018](adr/0018-the-markets-features-and-the-triage.md),
[ADR-0022](adr/0022-the-triage-row-by-row.md), [ADR-0023](adr/0023-the-specification-accepted.md)).
Done 2026-11-10.

### M1.4 — The cuts

What the triage cut left the egui client and the Android app, but three cuts kept until uwui
([ADR-0024](adr/0024-cuts-kept-in-the-egui-client-until-uwui.md)). Done 2026-11-10; its start is to
measure again on an idle machine.

### M1.5 — Agents' sessions recorded

The recorder, the fake agent, the masking and the replay into the fork, in
`crates/pluto-recording` ([ADR-0025](adr/0025-agents-sessions-recorded-and-replayed.md)). Built
2026-11-11; 2 of 11 real recordings made. The nine left move to Stage 4, with the Claude Code
engine and an ACP agent.

### M1.6 — The rivals measured

The fake model service and the rivals' bench
([ADR-0026](adr/0026-the-rivals-measured-how.md) to
[ADR-0029](adr/0029-the-benchs-tools-and-what-a-rivals-figure-holds.md)). M1.6a and M1.6b1 done
2026-11-11 and 2026-11-14, their figures provisional: the machine was loaded. M1.6b2 (the open
rivals and the web chats), M1.6b3 (the long thread, real days) and M1.6c (the vendors'
applications, energy, frames, the bar tightened) move to Stage 5. The unpushed branch `m16b2b`
holds part of M1.6b2; its fate is a question for Stage 1's audit.
