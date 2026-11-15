# 0037. How the harness is judged

- Status: Accepted; amends [0017](0017-pluto-measured-against-the-market.md) (the harness joins
  the bar, with its own rule)
- Date: 2026-11-15

## Context

The owner wants Pluto judged on public benchmarks, context cost, speed and resources, with local
models equal for all. What the surveys of 2026-11-15 found:

- **At the top, the model sets the ceiling.** On Terminal-Bench 4.1 (66 tasks, five trials each,
  [board](https://www.tbench.ai/leaderboard/terminal-bench/2.0) of 2026-11-14), the first four rows
  overlap within ±3 points. Opus 5.5 scores 65.8 in Claude Code and 64.6 on another runner
  ([Artificial Analysis](https://artificialanalysis.ai/agents/coding)).
- **The harness moves small models most**: up to 27 points
  ([ADR-0011](0011-plutos-own-harness-minimal-and-measured.md)).
- **Benchmarks are noisy.** Infrastructure settings alone moved Terminal-Bench 2.0 by about 6
  points ([Anthropic](https://www.anthropic.com/engineering/infrastructure-noise)). Thirty tasks
  resolve only differences of 15 to 20 points. Tasks are scored 0 for reward hacking, and agents
  were removed from the board for cheating.
- **A full run is dear**: one Terminal-Bench 4.1 run costs thousands of dollars on a frontier model
  at API prices, a few hundred on a cheap one (the board's own figures).
- **Vendors ship regressions.** Three changes to Claude Code hurt its quality for weeks in 2026,
  and its evals did not show them at first
  ([postmortem](https://www.anthropic.com/engineering/april-23-postmortem)).
- **The owner's work** already has saved briefs, each with hidden checks the agent never saw.

## Decision

**On frontier models**, Pluto's loop matches the vendor's own harness within the error margin, and
spends fewer tokens.

**On local and cheap models**, Pluto's loop leads, by a measured margin.

**Measured on two task sets:**
- **Pluto's bench**: saved briefs replayed, each graded by its hidden checks;
- **a subset of Terminal-Bench**, for a public reference.

Every result carries its confidence interval. The bench's environment is fixed and recorded, as
part of what is measured.

**Within a fixed monthly budget for runs**, which the owner sets. Agents run local models through
Ollama by hand during a milestone. Runs on API keys or subscriptions are the owner's, or need the
owner's go.

**Every component of the loop is kept only if the bench shows its gain**
([ADR-0032](0032-plutos-loop-and-its-provider-clients.md)). Tokens per task, cache hits, time and
cost are measured from the first run.

**The bench gates changes.** A change to the loop, a model profile or a vendor's new version is
replayed before it reaches the owner's work.

## Alternatives considered

- **Beat the vendors on frontier models, on the public boards.** The strongest claim. But the model
  sets the ceiling, and the noise is larger than any likely lead.
- **Public boards only.** Comparable with others. But noisy, dear, and blind to the owner's own
  work.
- **Pluto's bench only.** Cheap and close to the owner's work. But no outside reference.

## Consequences

- The bench comes first, in Stage 2 ([ADR-0039](0039-the-order-of-the-stages.md)), before the loop
  it judges.
- Its task set, its subset of Terminal-Bench, the runs per task and the monthly budget are decided
  at Stage 2's start. Saved briefs from private projects cannot be published: what of them may sit
  in this repository is decided then.
- [ADR-0017](0017-pluto-measured-against-the-market.md)'s token budget holds for vendors' engines:
  a turn started by Pluto spends at most 5 % more tokens than the same agent alone.
