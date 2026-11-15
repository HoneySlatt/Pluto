# 0039. The order of the stages, and Stage 1 closed

- Status: Accepted; amends [0017](0017-pluto-measured-against-the-market.md) and
  [0027](0027-the-rivals-measures-set-aside.md) (the rivals' remaining campaigns move to Stage 5)
- Date: 2026-11-15

## Context

The ROADMAP of 2026-11-08 built the server first (Stage 2), every agent on it (Stage 3), Pluto's own
agent (Stage 4), several machines (Stage 5), the client on uwui (Stage 6), the editor (Stage 7),
then going public. Pluto now becomes a harness ([ADR-0030](0030-pluto-becomes-a-harness.md)), and
the harness is judged on a bench before anything is built on it
([ADR-0037](0037-how-the-harness-is-judged.md)).

Where Stage 1 stands on 2026-11-15:
- M1.1 to M1.4 are done; the HP's figures wait for the owner's hand.
- M1.5 built the recorder and the replay; 2 of 11 real recordings are made.
- M1.6a and M1.6b1 built the fake model and the rivals' bench; the rivals' figures are
  provisional, taken on a loaded machine. M1.6b2, M1.6b3 and M1.6c are not run. The branch
  `m16b2b`, unpushed, holds part of M1.6b2's work.

A challenger proposed an order: the bench first, then Pluto's own agent, the engines, the server,
uwui, the assistant. The owner said that uwui will be ready before Stage 3.

## Decision

**The stages, in order**, one step of the design's order each (the owner's choice of
2026-11-15):
1. **Stage 1**, the ground measured and triaged, closes now, after its stage audit.
2. **Stage 2**: the bench and the session log's format.
3. **Stage 3**: `pluto-agent` and its TUI: Pluto's loop, its provider clients, the sandbox, a
   vault, the delegation object. The owner's delegated tasks move first.
4. **Stage 4**: the engines: Claude Code with the mod, Codex, ACP; quotas and exact costs.
5. **Stage 5**: the server, the protocol, pairing, the scheduler, the owner's inbox; the base of the
   uwui client and the Android app on them; the history imported; the fork stopped
   ([ADR-0038](0038-a-new-foundation-beside-the-fork.md)).
6. **Stage 6**: the uwui client completed: the chat, the dashboard, design.
7. **Stage 7**: the personal assistant.
8. **Stage 8**: going public.

**What Stage 1 leaves moves:**
- M1.5's nine recordings left go to Stage 4, with the Claude Code engine and an ACP agent.
- M1.6b2, M1.6b3 and M1.6c go to Stage 5, once a Pluto client exists on Pluto's own server to
  measure.
- The fate of the branch `m16b2b` is a question for Stage 1's audit.

## Alternatives considered

- **The server first**, as the ROADMAP of 2026-11-08. The fork would stop sooner. But the server
  would be shaped before the harness that it serves exists.
- **The bench and the loop in one stage.** One stage fewer, ending on daily use. But a long stage
  with no exit between, and a loop built before the yardstick that judges it.
- **The uwui client whole at Stage 6, the fork kept until then.** Node runs one stage longer.
- **The uwui client from Stage 3, beside the TUI.** A desktop client sooner. But two surfaces to
  keep from Stage 3, and a move to the server's protocol to make again at Stage 5.

## Consequences

- The editor's stage leaves the ROADMAP ([ADR-0040](0040-out-of-the-plan.md)).
- The rivals are measured again at each stage's end from Stage 5, each figure with its version and
  date, as [ADR-0017](0017-pluto-measured-against-the-market.md) asks.
- Stage 1 moves from the PLAN to `docs/history.md` now; its audit follows, in a session of its own,
  before Stage 2's first milestone.
