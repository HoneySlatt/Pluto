# 0034. The method's mechanisms native, the method itself in text

- Status: Accepted
- Date: 2026-11-15

## Context

The owner works with agents through a method of their own: rules in each project's `AGENTS.md`,
milestones in its PLAN, and a delegation skill. The skill names roles (lead, authors, reader,
web research, checker, reviewers, challenger), a table of the models for each role, briefs saved
with their base commit, hidden checks the child never sees, proofs kept as files, and a ledger of
every task. It is already a harness, made of scripts over six agent tools.

Its ledger held 467 tasks on 2026-11-15. A survey of it found:
- the bookkeeping is by hand: each brief, each hidden check, each ledger line;
- 74 tasks record a report the child could not write; in 37 the lead saved it instead;
- 15 tasks were cut by a usage limit and 43 resumed, the provider marked down by hand;
- most of the spend on Claude is context read again, not routing;
- every child runs as the owner, and could read another provider's credentials and the hidden
  checks.

The owner's own audit of the method asked for: efforts per role tuned by replays; a checker only
where it pays, with its cost in the ledger; explicit end conditions and a detection of false
stops; a time budget given to each child; mechanical limits on subagents; provenance on content;
the lead's exact cost; a reason on each rule.

The field:
- Codex, OpenCode and Claude Code each added a `/goal` within about six months: a completion
  condition, a checker and a budget.
- Cursor removed its planner, judge and integrator in turn as models improved
  ([Cursor](https://cursor.com/blog/self-driving-codebases)).
- The method's table of models changed four times on one day: a policy compiled into code would
  churn with it.
- A bounded-notes baseline matched three self-improvement methods with 65.6 to 71.0 % fewer tokens
  (arXiv 2611.13524, a preprint).

## Decision

**The mechanisms are native**, in Pluto's kernel:
- **the delegation object**: the brief, its base commit, the hidden checks kept in the vault out of
  the child's reach, the proofs, the report written by Pluto, and the ledger line, in one record;
- limits of depth, count and time;
- end conditions, and false stops refused;
- the provenance of content: the owner's, an agent's, a tool's, untrusted; the lead's choice,
  from OpenClaw's memory and shown to the owner with this record: it is set by code, never by the
  model;
- exact costs, from each provider's usage;
- providers down, fallback, and resume from the last step.

**The method itself stays in text files**, read when a task starts: the roles, the table of models,
the efforts, when to check, named agents, skills and hooks. Changing the method changes a file, not
Pluto.

**A know-how is recorded as a skill file**, never learnt by Pluto alone. Pluto proposes the file;
the owner reviews it before it is kept.

## Alternatives considered

- **The method compiled into Pluto.** The fastest to run. But it would churn with every change of
  the method, four times a day at worst.
- **Everything stays in scripts.** Nothing to build. But the bookkeeping, the lost reports, the
  limits noticed by hand and the shared credentials stay.
- **A self-improving loop**, as Hermes's. Agents that learn alone. But bounded notes did as well
  for a third of the tokens, and evolving on one task set has destroyed another (a preprint saw
  SWE-Bench fall from 94.0 % to 32.0 %).

## Consequences

- The owner's delegated tasks are the first slice of daily work to move onto Pluto, in Stage 3.
- Pluto reads the method's files where the owner keeps them. It writes none of them itself: a new
  skill is a proposal the owner keeps or not.
- Efforts per role and the conditional checker become figures on the bench
  ([ADR-0037](0037-how-the-harness-is-judged.md)), from replays of saved briefs.
